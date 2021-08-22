use chrono::Utc;
use futures::future::try_join_all;
use redis::AsyncCommands;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

#[tokio::test]
async fn try_lock() {
  let redis = redis::Client::open("redis://127.0.0.1/").unwrap();
  let relock = Relock::new("redis://127.0.0.1/").unwrap();

  let key = "bariloche";
  let ttl = Duration::from_secs(1).as_millis() as usize;

  // Assert lock creation
  let lock = relock.try_lock(key, ttl).await.unwrap();
  let actual = lock.id.len();
  let expected = 20;
  assert_eq!(actual, expected);

  let mut con = redis.get_async_connection().await.unwrap();

  // Assert Redis key creation
  let value: String = con.get(key).await.unwrap();
  let actual = value;
  let expected = lock.id;
  assert_eq!(actual, expected);

  // Assert key TTL
  sleep(Duration::from_secs(1)).await;
  let value = con.get::<&str, RedisValue>(key).await;
  let actual = value;
  let expected = Ok(RedisValue::Nil);
  assert_eq!(actual, expected);
}

#[tokio::test]
async fn unlock() {
  let redis = redis::Client::open("redis://127.0.0.1/").unwrap();
  let relock = Relock::new("redis://127.0.0.1/").unwrap();

  let key = "buenosaires";
  let ttl = Duration::from_secs(1).as_millis() as usize;

  let lock = relock.try_lock(key, ttl).await.unwrap();
  let lock_id = lock.id;

  // Assert lock removal
  let response = relock.unlock(key, &lock_id).await.unwrap();
  let actual = response;
  let expected = 1;
  assert_eq!(actual, expected);

  let mut con = redis.get_async_connection().await.unwrap();

  // Assert Redis key removal
  let value = con.get::<&str, RedisValue>(key).await;
  let actual = value;
  let expected = Ok(RedisValue::Nil);
  assert_eq!(actual, expected);

  // Assert lock removal for the second time
  let response = relock.unlock(key, &lock_id).await.unwrap();
  let actual = response;
  let expected = 0;
  assert_eq!(actual, expected);
}

#[tokio::test]
async fn try_lock_when_lock_is_used() {
  let relock = Relock::new("redis://127.0.0.1/").unwrap();

  let key = "mendoza";
  let ttl = Duration::from_secs(2).as_millis() as usize;

  // Assert lock creation
  let lock = relock.try_lock(key, ttl).await.unwrap();
  let actual = lock.id.len();
  let expected = 20;
  assert_eq!(actual, expected);

  // Try to use the lock when lock is alredy being used
  let lock = relock.try_lock(key, ttl).await;
  assert!(lock.is_err());
  assert_eq!(
    lock.unwrap_err(),
    Error::CanNotGetLock(error::CanNotGetLockReason::LockIsBussy)
  );
}

// This test creates multiple tasks to write a file. This test scenario
// assumes the file can be written by one task at a given time. Every task
// executes on a different thread and tries to get the lock concurrently. When
// the task gets the lock, it writes the file and then drops the lock. When
// the task drops the lock, another task picks it. Then, the test asserts the
// file was written x amount of time every y times accordingly.
#[tokio::test]
async fn lock_concurrently() {
  const TASKS_COUNT: usize = 10;

  match tokio::fs::remove_file("test.txt").await {
    Ok(_) => {}
    Err(ref err) if err.kind() == std::io::ErrorKind::NotFound => {}
    Err(err) => panic!("Failed to remove test file{}", err),
  };

  let tasks = (0..TASKS_COUNT).collect::<Vec<_>>().into_iter().map(|_| {
    tokio::spawn(async move {
      let relock = Relock::new("redis://127.0.0.1/").unwrap();

      let key = "ushuaia";
      let ttl = Duration::from_secs(1).as_millis() as usize;
      let retry_count = 20;
      let retry_delay = 1000;

      let lock = relock
        .lock(key, ttl, retry_count, retry_delay)
        .await
        .unwrap();

      write_timestamp_to_file("test.txt").await.unwrap();
      relock.unlock(key, lock.id).await.unwrap();
    })
  });

  try_join_all(tasks).await.unwrap();

  let mut file = fs::File::open("test.txt").await.unwrap();
  let mut buffer = vec![];
  file.read_to_end(&mut buffer).await.unwrap();

  let timestamps = std::str::from_utf8(buffer.as_ref())
    .unwrap()
    .split("\n")
    .filter(|timestamp| !timestamp.is_empty())
    .map(|timestamp| timestamp.parse::<i64>().unwrap())
    .collect::<Vec<_>>();

  // Assert the file was written 10 times.
  assert_eq!(timestamps.len(), TASKS_COUNT);

  // Assert the file was written every one second.
  for chunk in timestamps.chunks(2) {
    let actual = chunk[1] - chunk[0];
    let expected = 1;
    assert_eq!(actual, expected)
  }
}

async fn write_timestamp_to_file(path: &str) -> Result<(), std::io::Error> {
  let mut file = fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(path)
    .await?;

  let now = Utc::now();

  file
    .write_all(format!("{:?}\n", now.timestamp()).as_bytes())
    .await?;

  sleep(Duration::from_secs(1)).await;
  Ok(())
}
