//! A worker pool for independent pieces of a computation.
//!
//! Natively the work is split over threads (`std::thread::scope`), one chunk per thread, with the
//! number of threads from `DIAGCALC_THREADS` or the machine's available parallelism. On WebAssembly
//! a module has one thread, so the same calls run sequentially; there the pool is a set of Web
//! Workers, each with its own module instance, and the front end splits a job across them (see
//! `docs/design.md`, "Worker pool"). Results always come back in input order, so a parallel run
//! gives exactly the answer of a sequential one.

/// The number of worker threads to use.
pub fn threads() -> usize {
    if cfg!(target_family = "wasm") {
        return 1;
    }
    if let Some(n) = std::env::var("DIAGCALC_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    {
        return n.max(1);
    }
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// `items.iter().map(f).collect()`, with the items split across worker threads. Below
/// `min_chunk` items per thread the work is not worth splitting and runs on the calling thread.
pub fn par_map<T: Sync, U: Send>(
    items: &[T],
    min_chunk: usize,
    f: impl Fn(&T) -> U + Sync,
) -> Vec<U> {
    let workers = threads().min(items.len() / min_chunk.max(1)).max(1);
    if workers == 1 {
        return items.iter().map(f).collect();
    }
    let chunk = items.len().div_ceil(workers);
    let f = &f;
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|c| s.spawn(move || c.iter().map(f).collect::<Vec<U>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a worker thread panicked"))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_is_kept() {
        let v: Vec<u64> = (0..1000).collect();
        let out = par_map(&v, 10, |x| x * x);
        assert_eq!(out, v.iter().map(|x| x * x).collect::<Vec<_>>());
        assert!(par_map(&Vec::<u64>::new(), 1, |x| *x).is_empty());
    }
}
