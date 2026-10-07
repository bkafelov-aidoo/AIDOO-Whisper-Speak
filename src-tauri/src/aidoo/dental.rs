pub(crate) fn display_tooth(tooth: &str, is_milk_tooth: bool) -> String {
    if !is_milk_tooth || tooth == "*" {
        return tooth.to_string();
    }
    tooth
        .parse::<u8>()
        .ok()
        .filter(|number| matches!(number / 10, 1..=4) && matches!(number % 10, 1..=5))
        .map(|number| (number + 40).to_string())
        .unwrap_or_else(|| tooth.to_string())
}

pub(crate) fn spoken_tooth(tooth: &str) -> String {
    if tooth == "*" {
        return "звездичка".into();
    }
    let mut digits = tooth.chars();
    let Some(first) = digits.next().and_then(spoken_digit) else {
        return tooth.to_string();
    };
    let Some(second) = digits.next().and_then(spoken_digit) else {
        return tooth.to_string();
    };
    if digits.next().is_some() || !valid_display_tooth(tooth) {
        return tooth.to_string();
    }
    format!("{first} {second}")
}

pub(crate) fn tooth_sort_key(tooth: &str) -> (u8, u8, String) {
    let Ok(number) = tooth.parse::<u8>() else {
        return (u8::MAX, u8::MAX, tooth.to_string());
    };
    let quadrant = number / 10;
    let position = number % 10;
    let key = match quadrant {
        1 if matches!(position, 1..=8) => (0, 8 - position),
        2 if matches!(position, 1..=8) => (1, position - 1),
        3 if matches!(position, 1..=8) => (2, 8 - position),
        4 if matches!(position, 1..=8) => (3, position - 1),
        5 if matches!(position, 1..=5) => (4, 5 - position),
        6 if matches!(position, 1..=5) => (5, position - 1),
        7 if matches!(position, 1..=5) => (6, 5 - position),
        8 if matches!(position, 1..=5) => (7, position - 1),
        _ => return (u8::MAX, u8::MAX, tooth.to_string()),
    };
    (key.0, key.1, String::new())
}

fn valid_display_tooth(tooth: &str) -> bool {
    tooth.parse::<u8>().is_ok_and(|number| {
        let quadrant = number / 10;
        let position = number % 10;
        (matches!(quadrant, 1..=4) && matches!(position, 1..=8))
            || (matches!(quadrant, 5..=8) && matches!(position, 1..=5))
    })
}

fn spoken_digit(digit: char) -> Option<&'static str> {
    match digit {
        '1' => Some("едно"),
        '2' => Some("две"),
        '3' => Some("три"),
        '4' => Some("четири"),
        '5' => Some("пет"),
        '6' => Some("шест"),
        '7' => Some("седем"),
        '8' => Some("осем"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_fdi_numbers_digit_by_digit() {
        assert_eq!(spoken_tooth("18"), "едно осем");
        assert_eq!(spoken_tooth("21"), "две едно");
        assert_eq!(spoken_tooth("55"), "пет пет");
        assert_eq!(spoken_tooth("*"), "звездичка");
    }

    #[test]
    fn sorts_teeth_in_clinical_status_sequence() {
        let mut teeth = vec!["41", "11", "28", "38", "18", "21", "31", "48"];
        teeth.sort_by_key(|tooth| tooth_sort_key(tooth));
        assert_eq!(teeth, ["18", "11", "21", "28", "38", "31", "41", "48"]);
    }
}
