fn authorize(user: &str) -> bool { !user.is_empty() }

fn run(u: &str) -> bool { authorize(u) }
