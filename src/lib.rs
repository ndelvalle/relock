//! Distributed async locking using Redis.
//!
//! ### Description
//!
//! Distributed locks are a very useful primitive in many environments where
//! different processes must operate with shared resources in a mutually
//! exclusive way.
//! This crate implements the "Correct implementation with a single instance"
//! algorithm described in the [Redis documentation](https://redis.io/topics/distlock#correct-implementation-with-a-single-instance).
//!
//! ### lock
//!
//! ```
//! use relock::Relock;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let relock = Relock::new("redis://127.0.0.1/").unwrap();
//!
//! let lock_key = "foo-lock";
//! let time_to_live = 10_000;
//! let retry_count = 5;
//! let retry_delay = 200;
//!
//! // Acquire the lock. If the lock is bussy, this method will retry
//! // `retry_count` times with a delay of `retry_delay` milliseconds between
//! // each retry.
//! let lock = relock.lock(lock_key, time_to_live, retry_count, retry_delay).await.unwrap();
//! // Do something
//! relock.unlock(lock_key, lock.id).await.unwrap();
//! # }
//! ```
//!
//!
//! ### try_lock
//!
//! ```
//! use relock::Relock;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let relock = Relock::new("redis://127.0.0.1/").unwrap();
//!
//! let lock_key = "foo-try-lock";
//! let time_to_live = 10_000;
//!
//! // Acquire the lock. If the lock is bussy, this method will return a Lock
//! // Error. Consider waiting a bit before retrying or use `lock` method instead.
//! let lock = relock.try_lock(lock_key, time_to_live).await.unwrap();
//! // Do something
//! relock.unlock(lock_key, lock.id).await.unwrap();
//! # }
//! ```

// Implementation details:
// https://redis.io/topics/distlock#correct-implementation-with-a-single-instance

mod error;

use rand::Rng as RngTrait;
use redis::Client as RedisClient;
use redis::IntoConnectionInfo;
use redis::Value as RedisValue;
use tokio::time::{sleep, Duration};

pub use error::Error;

const LOCK_SCRIPT: &str = "return redis.call('set', ARGV[1], ARGV[2], 'px', ARGV[3], 'nx')";
const UNLOCK_SCRIPT: &str = r#"
  if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
  else
    return 0
  end
"#;

#[derive(Debug)]
pub struct Lock {
  pub id: String,
}

#[derive(Clone)]
pub struct Relock {
  client: RedisClient,
}

impl Relock {
  pub fn new<T: IntoConnectionInfo>(params: T) -> Result<Self, Error> {
    let client = redis::Client::open(params)?;
    Ok(Self { client })
  }

  pub async fn try_lock<T: AsRef<str>>(&self, key: T, ttl: usize) -> Result<Lock, Error> {
    let mut con = self.client.get_async_connection().await?;
    let id = create_random_string(20);
    let result = redis::Script::new(LOCK_SCRIPT)
      .arg(key.as_ref())
      .arg(&id)
      .arg(ttl)
      .invoke_async(&mut con)
      .await
      .map_err(Error::RedisError)?;

    match result {
      RedisValue::Okay => Ok(Lock { id }),
      _ => Err(Error::CanNotGetLock(
        error::CanNotGetLockReason::LockIsBussy,
      )),
    }
  }

  pub async fn lock<T>(
    &self,
    key: T,
    ttl: usize,
    retry_count: u32,
    retry_delay: u32,
  ) -> Result<Lock, Error>
  where
    T: AsRef<str>,
  {
    for _ in 0..retry_count {
      let lock_result = self.try_lock(key.as_ref(), ttl).await;
      match lock_result {
        Ok(lock) => return Ok(lock),
        Err(Error::RedisError(error)) => return Err(Error::RedisError(error)),
        Err(Error::CanNotGetLock(_)) => {
          // Consider adding a jitter.
          // https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
          sleep(Duration::from_millis(u64::from(retry_delay))).await;
          continue;
        }
      };
    }

    Err(Error::CanNotGetLock(
      error::CanNotGetLockReason::LockIsStillBusy {
        retry_count,
        retry_delay,
      },
    ))
  }

  pub async fn unlock<K, V>(&self, key: K, lock_id: V) -> Result<i64, Error>
  where
    K: AsRef<str>,
    V: AsRef<str>,
  {
    let mut con = self.client.get_async_connection().await?;
    let result: RedisValue = redis::Script::new(UNLOCK_SCRIPT)
      .key(key.as_ref())
      .arg(lock_id.as_ref())
      .invoke_async(&mut con)
      .await?;

    match result {
      RedisValue::Int(remove_count) => Ok(remove_count),
      _ => Ok(0),
    }
  }
}

fn create_random_string(size: usize) -> String {
  rand::thread_rng()
    .sample_iter(&rand::distributions::Alphanumeric)
    .take(size)
    .map(char::from)
    .collect()
}

#[cfg(test)]
mod lib_test;
