pub fn total(rows: &[i64]) -> i64 {
    let mut sum = 0;
    for row in rows {
        sum = crate::math::add(sum, *row);
    }
    sum
}
