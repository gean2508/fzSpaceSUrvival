//! Nombres pronunciables, sorteados de un hash (nada de tablas de 500 nombres).

use super::azar::hash;

const SIL: [&str; 32] = [
    "ka", "ra", "mi", "to", "sa", "el", "an", "or", "ve", "lu", "da", "ni", "po", "ha", "ju", "ma",
    "re", "si", "ta", "ze", "bo", "fa", "gi", "yu", "ne", "co", "li", "dro", "mar", "tel", "quin", "bra",
];

fn mayus(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn silabas(h: u64, n: usize) -> String {
    let mut s = String::new();
    let mut x = h;
    for _ in 0..n {
        x = hash(x);
        s.push_str(SIL[(x % SIL.len() as u64) as usize]);
    }
    s
}

pub fn persona(mujer: bool, h: u64) -> String {
    let n = 2 + (hash(h ^ 0xA1) % 2) as usize;
    let mut s = silabas(h, n);
    let fin = if mujer { "a" } else { ["o", "n", "s", "r"][(hash(h ^ 0xB2) % 4) as usize] };
    if mujer {
        if !s.ends_with('a') {
            s.push_str(fin);
        }
    } else if s.ends_with('a') {
        s.pop();
        s.push_str(fin);
    }
    mayus(&s)
}

pub fn casa(h: u64) -> String {
    mayus(&silabas(h ^ 0xC3, 2 + (hash(h ^ 0xC4) % 2) as usize))
}

pub fn pueblo(h: u64) -> String {
    let base = mayus(&silabas(h ^ 0xD5, 2 + (hash(h ^ 0xD6) % 2) as usize));
    match hash(h ^ 0xD7) % 6 {
        0 => format!("San {base}"),
        1 => format!("{base} del Valle"),
        2 => format!("Puerto {base}"),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn son_deterministas_y_no_vacios() {
        for h in 0..300u64 {
            assert!(!persona(h % 2 == 0, h).is_empty());
            assert_eq!(pueblo(h), pueblo(h));
            assert!(casa(h).chars().next().unwrap().is_uppercase());
        }
    }
}
