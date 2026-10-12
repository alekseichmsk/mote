fn main() {
    let aff = std::fs::read_to_string("resources/dictionaries/ru_RU/ru_RU.aff").unwrap();
    let dic = std::fs::read_to_string("resources/dictionaries/ru_RU/ru_RU.dic").unwrap();
    let t = std::time::Instant::now();
    let dict = spellbook::Dictionary::new(&aff, &dic).expect("dict");
    println!("load {:?}", t.elapsed());
    println!("privet_ok {}", dict.check("привет"));
    println!("prevet_ok {}", dict.check("превет"));
    let mut s = Vec::new();
    dict.suggest("превет", &mut s);
    println!("suggest {:?}", &s[..s.len().min(8)]);
}
