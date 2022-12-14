#[macro_use]
extern crate lazy_static;

use std::env;
//TODO remove this hack when rayon fixes itself

lazy_static! {
    // reduce the number of threads each pool is allowed to half the cpu core count, to avoid rayon
    // hogging cpu
    static ref MAX_RAYON_THREADS: usize =
            env::var("SOLANA_RAYON_THREADS").ok()
            .and_then(|num_threads| num_threads.parse().ok())
            .unwrap_or_else(|| num_cpus::get() as usize / 2)
            .max(1);
}

pub fn get_thread_count() -> usize {
    *MAX_RAYON_THREADS
}
