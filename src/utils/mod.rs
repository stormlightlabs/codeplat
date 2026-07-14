pub fn escape_inline_code(input: &str) -> String {
    sanitize_text(input).replace('`', "\\`")
}

pub fn escape_markdown(input: &str) -> String {
    let mut output = String::new();
    for character in sanitize_text(input).chars() {
        if matches!(character, '\\' | '*' | '_' | '[' | ']') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

pub fn sanitize_text(input: &str) -> String {
    input
        .chars()
        .map(|character| if character.is_control() { '�' } else { character })
        .collect()
}
