# ephemeral-cache

`ephemeral-cache` is a small last-resort cache shared by the statsapi clients.
Clients try the live API first; the cache only answers when the live call
fails, and every entry expires (TTL) and the directory is size-capped.

## Install

```toml
[dependencies]
ephemeral-cache = { git = "https://github.com/davidawad/statsapi", package = "ephemeral-cache" }
```

## Use

```rust
use ephemeral_cache::{EphemeralCache, LIVE_TTL, DEFAULT_CAP_BYTES};

let cache = EphemeralCache::new("/tmp/my-client-cache", LIVE_TTL, DEFAULT_CAP_BYTES);
cache.put("key", b"body")?;
let hit = cache.get("key"); // None once the entry is older than the TTL
# Ok::<(), std::io::Error>(())
```

- `EphemeralCache::answer(key, live)` runs the live closure, stores its body,
  and falls back to a fresh cached entry only if the live call fails.
- `Trail` records which `Source` (live, stream, cache) answered a request.
- `BULK_TTL` (7 days) for bulk downloads, `LIVE_TTL` (1 hour) for API calls,
  `DEFAULT_CAP_BYTES` (256 MiB) per cache directory.
- `write_atomic` and `evict_oldest` for callers that keep their own layout.

The cache directory is chosen by the caller; nothing is written unless a client
is given one.

## License

GPL-3.0-or-later.
