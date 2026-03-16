pub struct Calculator;

impl Calculator {
    pub fn add(&self, a: i32, b: i32) -> i32 {
        let result = a + b;
        result
    }

    pub fn multiply(&self, a: i32, b: i32) -> i32 {
        a.checked_mul(b).unwrap_or(i32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let calc = Calculator;
        assert_eq!(calc.add(2, 3), 5);
        assert_eq!(calc.add(-1, 1), 0);
        assert_eq!(calc.add(0, 0), 0);
    }

    #[test]
    fn test_multiply() {
        let calc = Calculator;
        assert_eq!(calc.multiply(2, 3), 6);
        assert_eq!(calc.multiply(-1, 5), -5);
        assert_eq!(calc.multiply(i32::MAX, 2), i32::MAX);
    }
}
