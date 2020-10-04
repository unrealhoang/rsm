pub fn quorum_match_index<I>(elements: I) -> u64
where
    I: ExactSizeIterator<Item = u64> + Clone,
{
    let outer = elements.clone();
    let majority_threshold = elements.len() / 2 + 1;

    let mut min_accepted = None;
    for current in outer {
        let mut count_lte = 0;
        let inner = elements.clone();
        for item in inner {
            if current >= item {
                count_lte += 1;
            }
        }
        println!(
            "Debug current: {}, count: {}, majority_threshold: {}",
            current, count_lte, majority_threshold
        );
        if count_lte >= majority_threshold
            && (min_accepted.is_none() || current < min_accepted.unwrap())
        {
            min_accepted = Some(current)
        }
        println!("Min accepted: {:?}", min_accepted);
    }
    min_accepted.unwrap()
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::Rng;

    fn random_vec(rng: &mut StdRng) -> Vec<u64> {
        let vec_size = rng.gen_range(2, 20);
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

        for _i in 0..1000 {
            let vec = random_vec(&mut rng);
            let mut sorted_vec = vec.clone();
            sorted_vec.sort();

            let median = sorted_vec[vec.len() / 2];
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
