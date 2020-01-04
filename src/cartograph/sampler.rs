use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

pub trait Sample<I>: Iterator<Item = I>
where
    I: Hash,
{
    /// Sample from the iterator, returning at most `max_num` elements.
    /// By design, this is a stable sampler, that is, the result is not dependent
    /// on the order in which the values arrive from the iterator.
    ///
    /// If there are less than `max_num` elements in total, they will be returned
    /// intact. Otherwise, due to the way the logic is implemented any value from
    /// 0 to `max_num` can be returned.
    fn sample(self, max_num: usize) -> Vec<I>;
}

impl<I: Hash, T: Iterator<Item = I>> Sample<I> for T {
    fn sample(self, max_num: usize) -> Vec<I> {
        let mut sampler = Sampler::new(max_num);
        for el in self {
            sampler.update(el);
        }
        sampler.finish()
    }
}

pub trait PrioritySample<I>: Iterator<Item = I>
where
    I: Hash,
{
    /// Sample from the iterator, returning at most `max_num` elements with the highest
    /// priority.
    /// By design, this is a stable sampler, that is, the result is not dependent
    /// on the order in which the values arrive from the iterator.
    ///
    /// If there are less than `max_num` elements in total, they will be returned
    /// intact. Otherwise, due to the way the logic is implemented any value from
    /// 0 to `max_num` can be returned.
    ///
    /// If in total, there are more than `max_num` elements with priority larger than
    /// `k`, then no element with a lower priority will be returned
    fn sample_with_priority<F>(self, max_num: usize, get_priority: F) -> BTreeMap<i32, Vec<I>>
    where
        F: Fn(&I) -> i32;
}

impl<I: Hash, T: Iterator<Item = I>> PrioritySample<I> for T {
    fn sample_with_priority<F>(self, max_num: usize, get_priority: F) -> BTreeMap<i32, Vec<I>>
    where
        F: Fn(&I) -> i32,
    {
        let mut samplers = BTreeMap::new();
        let mut min_priority = std::i32::MIN;

        for el in self {
            let priority = get_priority(&el);

            if priority < min_priority {
                // We know it is useless to handle this element, since it will not
                // be returned
                continue;
            }

            // Find corresponding sampler
            let sampler = samplers
                .entry(priority)
                .or_insert_with(|| Sampler::new(max_num));
            sampler.update(el);

            if sampler.len() >= max_num {
                // Just this priority level alone could answer the full query
                min_priority = priority;
            }
        }

        // Collect results from samplers
        let mut result = BTreeMap::new();
        let mut total_els = 0;
        for (priority, mut sampler) in samplers.into_iter().rev() {
            sampler.resample(max_num - total_els);
            total_els += sampler.len();
            result.insert(priority, sampler.finish());
            if total_els >= max_num {
                break;
            }
        }
        result
    }
}

struct Sampler<T: Hash> {
    // Store kept values along with their hashes
    result: Vec<(T, u64)>,
    max_num: usize,
    hash_mask: u64,
    len: usize,
}

impl<T: Hash> Sampler<T> {
    fn new(max_num: usize) -> Self {
        Sampler {
            result: Vec::with_capacity(max_num),
            max_num,
            hash_mask: 0,
            len: 0,
        }
    }

    fn update(&mut self, el: T) {
        // Hash new element
        let mut hasher = DefaultHasher::new();
        el.hash(&mut hasher);
        let hash = hasher.finish();

        // Keep it
        if hash & self.hash_mask == 0 {
            // Make space for the new element
            while self.result.len() == self.max_num {
                // Drop approximately half of the elements by increasing by one the number
                // of required zeros at the end of the hash
                self.hash_mask = (self.hash_mask << 1) + 1;
                let hash_mask = self.hash_mask;
                self.result.retain(|(_el, hash)| hash & hash_mask == 0);
            }

            // We need to recheck, since it may have change in the mean time
            if hash & self.hash_mask == 0 {
                self.result.push((el, hash));
            }
        }

        self.len += 1;
    }

    fn resample(&mut self, new_max_num: usize) {
        assert!(new_max_num <= self.max_num);
        self.max_num = new_max_num;
        while self.result.len() > self.max_num {
            // Drop approximately half of the elements by increasing by one the number
            // of required zeros at the end of the hash
            self.hash_mask = (self.hash_mask << 1) + 1;
            let hash_mask = self.hash_mask;
            self.result.retain(|(_el, hash)| hash & hash_mask == 0);
        }
    }

    fn finish(self) -> Vec<T> {
        self.result.into_iter().map(|(el, _hash)| el).collect()
    }

    fn len(&self) -> usize {
        self.len
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn multiple_compressions() {
        fn check(n1: usize, n2: usize, result: Vec<usize>) {
            dbg!(n1);
            assert_eq!((0..n1).sample(n2), result);
        }

        // All
        check(6, 100, vec![0, 1, 2, 3, 4, 5]);
        check(6, 6, vec![0, 1, 2, 3, 4, 5]);

        // Single compression
        check(6, 5, vec![2, 3, 5]);
        check(7, 5, vec![2, 3, 5, 6]);
        check(8, 5, vec![2, 3, 5, 6]);
        check(9, 5, vec![2, 3, 5, 6, 8]);

        // Double compression
        check(10, 5, vec![5, 6, 9]);
        check(11, 5, vec![5, 6, 9]);
        check(12, 5, vec![5, 6, 9]);
        for n in 13..23 {
            check(n, 5, vec![5, 6, 9, 12]);
        }
        for n in 23..30 {
            check(n, 5, vec![5, 6, 9, 12, 22]);
        }

        // Triple compression
        for n in 30..36 {
            check(n, 5, vec![6, 9, 12, 29]);
        }
        for n in 36..38 {
            check(n, 5, vec![6, 9, 12, 29, 35]);
        }

        // Quadruple compression
        for n in 38..106 {
            check(n, 5, vec![9]);
        }
    }

    #[test]
    fn stability() {
        fn check(values: Vec<usize>) {
            let mut result = values.into_iter().sample(5);
            result.sort();
            assert_eq!(result, vec![6, 9, 12, 29, 35]);
        }

        check((0usize..37).collect());

        check(vec![
            26usize, 8, 29, 14, 16, 15, 11, 30, 0, 24, 13, 25, 34, 3, 1, 27, 33, 28, 7, 5, 9, 21,
            10, 2, 23, 36, 4, 12, 20, 6, 31, 22, 35, 32, 19, 18, 17,
        ]);

        check(vec![
            26usize, 5, 2, 35, 13, 9, 6, 19, 0, 18, 4, 23, 15, 30, 25, 11, 14, 8, 24, 28, 33, 32,
            17, 12, 27, 34, 29, 10, 1, 20, 36, 31, 7, 3, 22, 21, 16,
        ]);
    }

    #[test]
    fn single_priority() {
        // Single priority works like the non-prioritized one
        assert_eq!(
            (0..38usize).sample_with_priority(5, |_| 0),
            vec![(0, (0..38usize).sample(5))].into_iter().collect()
        );
        assert_eq!(
            (0..100usize).sample_with_priority(5, |_| -17),
            vec![(-17, (0..100usize).sample(5))].into_iter().collect()
        );
    }

    #[test]
    fn saturated_priority() {
        // Only take from the top priorities if there are enough there
        assert_eq!(
            (0..38usize).sample_with_priority(5, |i| (i / 19) as i32),
            vec![(1, (19..38usize).sample(5))].into_iter().collect()
        );

        assert_eq!(
            (0..38usize).sample_with_priority(5, |i| (i / 10) as i32),
            vec![(3, (30..38usize).sample(5))].into_iter().collect()
        );

        let p3 = (30..38usize).sample(15);
        let p3_len = p3.len();
        assert_eq!(
            (0..38usize).sample_with_priority(15, |i| (i / 10) as i32),
            vec![(3, p3), (2, (20..30usize).sample(15 - p3_len))]
                .into_iter()
                .collect()
        );
    }
}
