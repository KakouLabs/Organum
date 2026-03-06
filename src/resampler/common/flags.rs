pub struct ParsedFlags {
    pub g: f64,
    pub b: f64,
    pub m: f64,
    pub t: f64,
    pub a: f64,
    pub p: f64,
    pub c: f64,
    pub h: f64,
    pub d: f64,
    pub f: f64,
}

impl Default for ParsedFlags {
    fn default() -> Self {
        Self {
            g: 0.0,
            b: 50.0,
            m: 0.0,
            t: 0.0,
            a: 100.0,
            p: 0.0,
            c: 0.0,
            h: 0.0,
            d: 0.0,
            f: 0.0,
        }
    }
}

pub fn parse_flags(s: &str) -> ParsedFlags {
    let mut flags = ParsedFlags::default();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_alphabetic() {
            let mut val_str = String::new();
            if let Some(&next) = chars.peek() {
                if next == '+' || next == '-' {
                    val_str.push(chars.next().unwrap());
                }
            }
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() || next == '.' {
                    val_str.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if let Ok(val) = val_str.parse::<f64>() {
                match c {
                    'g' | 'G' => flags.g = val,
                    'B' | 'b' => flags.b = val,
                    'M' => flags.m = val,
                    't' | 'T' => flags.t = val,
                    'A' | 'a' => flags.a = val,
                    'P' | 'p' | 'Y' | 'y' => flags.p = val,
                    'C' | 'c' => flags.c = val,
                    'H' | 'h' => flags.h = val,
                    'D' | 'd' => flags.d = val,
                    'F' | 'f' => flags.f = val,
                    _ => {}
                }
            }
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flags() {
        let flags = parse_flags("g10BB50M2.5t-12.5A2.0p1c50");
        assert_eq!(flags.g, 10.0);
        assert_eq!(flags.b, 50.0);
        assert_eq!(flags.m, 2.5);
        assert_eq!(flags.t, -12.5);
        assert_eq!(flags.a, 2.0);
        assert_eq!(flags.p, 1.0);
        assert_eq!(flags.c, 50.0);

        let overrides = parse_flags("H100 d-10 F+12");
        assert_eq!(overrides.h, 100.0);
        assert_eq!(overrides.d, -10.0);
        assert_eq!(overrides.f, 12.0);

        // Test default
        let def = parse_flags("unknown stuff");
        assert_eq!(def.b, 50.0);
        assert_eq!(def.a, 100.0);
    }
}
