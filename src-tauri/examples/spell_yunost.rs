fn main() {
    let aff = std::fs::read_to_string("resources/dictionaries/ru_RU/ru_RU.aff").unwrap();
    let dic = std::fs::read_to_string("resources/dictionaries/ru_RU/ru_RU.dic").unwrap();
    let dict = spellbook::Dictionary::new(&aff, &dic).unwrap();
    println!("yunost {}", dict.check("юность"));
    println!("yunast {}", dict.check("юнасть"));
    let mut s = Vec::new();
    dict.suggest("юнасть", &mut s);
    println!("{:?}", &s[..s.len().min(10)]);
}
