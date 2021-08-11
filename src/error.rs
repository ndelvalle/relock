use redis::RedisError;

#[derive(thiserror::Error, Debug)]
#[error("...")]
pub enum Error {
  #[error("{0}")]
  RedisError(#[from] RedisError),
  CanNotGetLock(CanNotGetLockVariants),
}

#[derive(Debug)]
pub enum CanNotGetLockVariants {
  LockIsBussy,
  LockTimeout,
}
