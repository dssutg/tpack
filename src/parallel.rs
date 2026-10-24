// parallel.rs - Small scoped-thread helpers used by the packing pipeline.
//
// The skyline packer itself is inherently sequential; the parallel work is
// sprite decoding and atlas compositing, which are independent per sprite
// (or per atlas row band). Both helpers preserve input order so the resulting
// atlas and sprite map are identical regardless of the thread count.

// map applies f to every item using up to `threads` worker threads and returns
// the results in the same order as items.
//
// threads is clamped to 1..=items.len(), so a request for more workers than
// work units never spawns idle threads. The single-worker path runs inline to
// avoid thread-spawn overhead.
pub fn map<T, R, F>(items: &[T], threads: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let workers = threads.max(1).min(items.len());
    if workers <= 1 {
        return items.iter().map(f).collect();
    }

    let chunk_size = items.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk_size)
            .map(|part| {
                let f = &f;
                scope.spawn(move || part.iter().map(f).collect::<Vec<R>>())
            })
            .collect();

        let mut out = Vec::with_capacity(items.len());
        for handle in handles {
            match handle.join() {
                Ok(part) => out.extend(part),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::map;

    #[test]
    fn preserves_order_for_every_thread_count() {
        let items: Vec<usize> = (0..97).collect();
        let expected: Vec<usize> = items.iter().map(|x| x * 2).collect();
        for threads in [1, 2, 3, 4, 8, 128] {
            assert_eq!(map(&items, threads, |x| x * 2), expected);
        }
    }

    #[test]
    fn handles_empty_input() {
        let items: Vec<usize> = Vec::new();
        assert!(map(&items, 4, |x| *x).is_empty());
    }
}
