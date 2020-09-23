pub fn quorum_match_index<I>(elements: I) -> u64
where
    I: ExactSizeIterator<Item = u64> + Clone,
{
    let outer = elements.clone();
    let majority_threshold = (elements.len() + 1) / 2;

    let mut max_quorum_match_index = 0;
    for current in outer {
        let mut count_lte = 0;
        let inner = elements.clone();
        for item in inner {
            if current <= item {
                count_lte += 1;
            }
        }
        if count_lte >= majority_threshold && current > max_quorum_match_index {
            max_quorum_match_index = current
        }
    }
    max_quorum_match_index
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::Rng;

    fn random_vec(rng: &mut StdRng) -> Vec<u64> {
        let vec_size = rng.gen_range(2, 15);
        (0..vec_size).map(|_| rng.gen_range(0, 50)).collect()
    }

    #[test]
    fn test_quorum_match_index() {
        use super::quorum_match_index;

        let data = vec![1, 2, 3, 4, 5];
        assert_eq!(3, quorum_match_index(data.iter().cloned()));
        let data = vec![1, 2, 3, 4];
        assert_eq!(3, quorum_match_index(data.iter().cloned()));
        let data = vec![1, 2, 2, 2];
        assert_eq!(2, quorum_match_index(data.iter().cloned()));
        let data = vec![1, 1, 2, 4];
        assert_eq!(2, quorum_match_index(data.iter().cloned()));
        let data = vec![5, 2, 3, 4, 5];
        assert_eq!(4, quorum_match_index(data.iter().cloned()));
    }

    #[test]
    fn test_quorum_match_index_random() {
        use super::quorum_match_index;
        use rand_core::SeedableRng;

        let mut rng = StdRng::seed_from_u64(12);

        for _i in 0..10 {
            let vec = random_vec(&mut rng);
            let mut sorted_vec = vec.clone();
            sorted_vec.sort();

            let median = sorted_vec[vec.len() - 1 / 2];
            assert_eq!(
                median,
                quorum_match_index(vec.iter().cloned()),
                "Vec: {:?}. Sorted: {:?}",
                vec,
                sorted_vec
            );
        }
    }
}
