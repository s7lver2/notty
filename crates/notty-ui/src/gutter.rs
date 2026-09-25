/// Ancho en píxeles de la columna de números de línea: dígitos del número más
/// largo (mínimo 1) por el ancho de un dígito, más 12px de margen a cada lado.
pub fn gutter_width(total_lines: usize, digit_width_px: f32) -> f32 {
    let digits = total_lines.max(1).to_string().len().max(1) as f32;
    digits * digit_width_px + 12.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_digit_lines() {
        assert_eq!(gutter_width(9, 10.0), 22.0);
    }

    #[test]
    fn three_digit_lines() {
        assert_eq!(gutter_width(120, 10.0), 42.0);
    }

    #[test]
    fn zero_lines_still_shows_one_digit() {
        assert_eq!(gutter_width(0, 10.0), 22.0);
    }
}
