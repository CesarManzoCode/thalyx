pub fn walk(roots: &[String]) -> usize {
    let mut seen = 0;
    for directory in roots {
        let directory = directory.trim();
        seen += directory.len();
    }
    seen
}
