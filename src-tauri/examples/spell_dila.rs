fn main() {
    let aff = std::fs::read_to_string("resources/dictionaries/ru_RU/ru_RU.aff").unwrap();
    let dic = std::fs::read_to_string("resources/dictionaries/ru_RU/ru_RU.dic").unwrap();
    let dict = spellbook::Dictionary::new(&aff, &dic).unwrap();
    for w in ["дила", "дела", "как", "превет"] {
        let mut s = Vec::new();
        dict.suggest(w, &mut s);
        println!("{w} check={} sug={:?}", dict.check(w), &s[..s.len().min(6)]);
    }
}
