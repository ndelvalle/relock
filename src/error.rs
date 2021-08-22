use redis::RedisError;

#[derive(thiserror::Error, Debug, PartialEq)]
#[error("...")]
pub enum Error {
  #[error("{0}")]
  RedisError(#[from] RedisError),
  CanNotGetLock(CanNotGetLockReason),
}

#[derive(Debug, PartialEq)]
pub enum CanNotGetLockReason {
  LockIsBussy,
  LockIsStillBusy { retry_count: u32, retry_delay: u32 },
}
