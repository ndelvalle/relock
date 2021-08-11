// https://redis.io/topics/distlock#correct-implementation-with-a-single-instance

pub mod error;

use rand::Rng as RngTrait;
use redis::Client as RedisClient;
use redis::Value as RedisValue;
use tokio::time::{sleep, Duration};

use error::Error;

pub struct Relock {
  client: RedisClient,
}

impl Relock {
  pub fn new(client: RedisClient) -> Self {
    Self { client }
  }

  pub async fn lock<T: AsRef<str>>(&self, key: T, ttl: usize) -> Result<LockResult, Error> {
    let mut con = self.client.get_async_connection().await.unwrap();
    let id = create_random_string(20);
    let result = set(&mut con, key, &id, ttl).await?;

    match result {
      RedisValue::Okay => Ok(LockResult { id }),
      RedisValue::Nil => Err(Error::CanNotGetLock(
        error::CanNotGetLockVariants::LockIsBussy,
      )),
      // Not sure if this case can ever happen.
      _ => Err(Error::CanNotGetLock(
        error::CanNotGetLockVariants::LockIsBussy,
      )),
    }
  }

  pub async fn unlock<K, V>(&self, key: K, id: V) -> Result<i64, Error>
  where
    K: AsRef<str>,
    V: AsRef<str>,
  {
    let mut con = self.client.get_async_connection().await.unwrap();
    let script = redis::Script::new(
      r#"
      if redis.call("get", KEYS[1]) == ARGV[1] then
        return redis.call("del", KEYS[1])
      else
        return 0
      end
    "#,
    );

    let result: RedisValue = script
      .key(key.as_ref())
      .arg(id.as_ref())
      .invoke_async(&mut con)
      .await
      .unwrap();

    match result {
      RedisValue::Int(remove_count) => Ok(remove_count),
      _ => Ok(0),
    }
  }

  pub async fn try_lock<T>(
    &self,
    key: &str,
    ttl: usize,
    max_attempts: i64,
    wait: u64,
  ) -> Result<LockResult, Error>
  where
    T: AsRef<str>,
  {
    let mut con = self.client.get_async_connection().await.unwrap();
    let id = create_random_string(20);

    for _ in 0..max_attempts {
      let set_lock_result = set(&mut con, key, &id, ttl).await;
      match set_lock_result {
        Ok(value) => value,
        Err(Error::RedisError(error)) => return Err(Error::RedisError(error)),
        Err(Error::CanNotGetLock(_)) => {
          sleep(Duration::from_millis(wait)).await;
          continue;
        }
      };
    }

    Err(Error::CanNotGetLock(
      error::CanNotGetLockVariants::LockTimeout,
    ))
  }
}

pub fn create_random_string(size: usize) -> String {
  rand::thread_rng()
    .sample_iter(&rand::distributions::Alphanumeric)
    .take(size)
    .map(char::from)
    .collect()
}

pub async fn set<K, V>(
  con: &mut redis::aio::Connection,
  key: K,
  value: V,
  ttl: usize,
) -> Result<RedisValue, Error>
where
  K: AsRef<str>,
  V: AsRef<str>,
{
  let script = r"return redis.call('set', ARGV[1], ARGV[2], 'PX', ARGV[3], 'NX')";
  redis::Script::new(script)
    .arg(key.as_ref())
    .arg(value.as_ref())
    .arg(ttl)
    .invoke_async(con)
    .await
    .map_err(Error::RedisError)
}

pub struct LockResult {
  id: String,
}

#[cfg(test)]
mod tests {
  use redis::AsyncCommands;

  use super::*;

  #[tokio::test]
  async fn set_lock_test() {
    let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    let relock = Relock {
      client: client.clone(),
    };

    let key = "foo";
    let ttl = Duration::from_secs(10).as_millis() as usize;

    let response = relock.lock(key, ttl).await.unwrap();
    assert_eq!(response.id.len(), 20);

    let mut con = client.get_async_connection().await.unwrap();
    let value: String = con.get(key).await.unwrap();
    assert_eq!(value, response.id);

    relock.unlock(key, response.id.as_str()).await.unwrap();
  }
}
