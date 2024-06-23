use rand::Rng;

fn generate_random_number_string(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| rng.gen_range(0..10))
        .map(|num| std::char::from_digit(num, 10).unwrap())
        .collect()
}

pub(crate) fn random_peer_id() -> String {
    generate_random_number_string(20)
}
