# relock

Distributed async locking using Redis

## Description

Distributed locks are a very useful primitive in many environments where
different processes must operate with shared resources in a mutually exclusive
way. This crate implements the "Correct implementation with a single instance"
algorithm described in the [Redis
documentation](https://redis.io/topics/distlock#correct-implementation-with-a-single-instance).


## Install

Add `relock` as a dependency in the cargo.toml file of your project:

```toml
[dependencies]
relock = "0.0.1"
```

If you have [cargo-edit](https://github.com/killercup/cargo-edit) utility tool
installed, use:

```bash
$ cargo add relock
```

## Usage

### lock

```rust
use relock::Relock;

let relock = Relock::new("redis://127.0.0.1/").unwrap();
let lock_key = "foo-lock";
let time_to_live = 10_000;
let retry_count = 5;
let retry_delay = 200;

// Acquire the lock. If the lock is bussy, this method will retry
// `retry_count` times with a delay of `retry_delay` milliseconds between
// each retry.
let lock = relock.lock(lock_key, time_to_live, retry_count, retry_delay).await.unwrap();
// Do work.
relock.unlock(lock_key, lock.id).await.unwrap();
```

### try_lock

```rust
use relock::Relock;

let relock = Relock::new("redis://127.0.0.1/").unwrap();
let lock_key = "foo-try-lock";
let time_to_live = 10_000;

// Acquire the lock. If the lock is bussy, this method will return a Lock
// Error. Consider waiting a bit before retrying or use `lock` method instead.
let lock = relock.try_lock(lock_key, time_to_live).await.unwrap();
// Do work.
relock.unlock(lock_key, lock.id).await.unwrap();
```
