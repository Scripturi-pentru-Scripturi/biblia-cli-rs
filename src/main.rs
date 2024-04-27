use clap::{Arg, Command};
use dotenv;
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::json;
use serde_json::{Map, Value};
use std::error::Error;
use std::fs;
use strsim::jaro_winkler;
use textwrap::{fill, Options};

fn find_book_name(bible: &Map<String, Value>, name: &str) -> String {
    let mut best_score = 0.0;
    let mut best_book = "";
    for (book_name, book_data) in bible.iter() {
        let score = jaro_winkler(name, book_name);
        if score > best_score {
            best_score = score;
            best_book = book_name;
        }
        if let Some(alternatives) = book_data.get("alternative").and_then(Value::as_array) {
            for alternative in alternatives.iter().filter_map(Value::as_str) {
                let score = jaro_winkler(name, alternative);
                if score > best_score {
                    best_score = score;
                    best_book = book_name;
                }
            }
        }
    }

    if best_score == 0.0 {
        eprintln!("Nu am gasit nici o carte cu numele '{}' in biblie.", name);
        return bible.keys().next().unwrap().to_string();
    }

    best_book.to_string()
}

fn get_verses(bible: &Map<String, Value>, book: &str, chapter: usize, start_verse: usize, end_verse: usize) -> Option<Vec<String>> {
    bible
        .get(book)
        .and_then(|book_data| book_data.get("capitole").and_then(Value::as_object))
        .and_then(|chapters| chapters.get(&chapter.to_string()).and_then(Value::as_object))
        .and_then(|verses_data| verses_data.get("versete").and_then(Value::as_array))
        .map(|verses| {
            verses
                .iter()
                .enumerate()
                .skip(start_verse - 1)
                .take(end_verse - start_verse + 1)
                .filter_map(|(_i, verse)| {
                    let verset_num = verse.get("verset").and_then(Value::as_u64)?;
                    let text = verse.get("text").and_then(Value::as_str)?;
                    Some(format!("{}:{} {}", chapter, verset_num, text))
                })
                .collect()
        })
}

fn parse_reference(reference: &str, llm: bool) -> (&str, usize, (usize, usize)) {
    let parts: Vec<&str> = reference.split(':').collect();
    let book_name = parts.get(0).unwrap_or_else(|| {
        eprintln!("Cartea nu e specificata. Foloseste ':' pentru a separa cartea de capitol si '-' pentru a separa versetele");
        std::process::exit(1);
    });

    let chapter_arg = parts.get(1).unwrap_or_else(|| {
        if llm {
            eprintln!("{}", reference);
            std::process::exit(1);
        }
        eprintln!("Capitolul nu e specificat. Foloseste ':' pentru a separa cartea de capitol si '-' pentru a separa versetele");
        std::process::exit(1);
    });
    let chapter: usize = chapter_arg.parse().unwrap_or_else(|_| {
        eprintln!("Capitolul nu e specificat. Foloseste ':' pentru a separa cartea de capitol si '-' pentru a separa versetele");
        std::process::exit(1);
    });
    let mut start_verse = 1;
    let mut end_verse = usize::MAX;
    if parts.len() == 3 {
        if let Some(dash) = parts[2].find('-') {
            let (start, end) = parts[2].split_at(dash);
            start_verse = start.parse().expect("Versetul de inceput nu e in format valid");
            end_verse = end[1..].parse().expect("Versetul de sfarsit nu e in format valid");
        } else {
            start_verse = parts[2].parse().expect("Versetul nu e in format valid");
            end_verse = start_verse;
        }
    } else if parts.len() > 3 {
        panic!("Referinta nu e in format valid, foloseste ':' pentru a separa capitolul de verset si '-' pentru a separa versetele");
    }
    (*book_name, chapter, (start_verse, end_verse))
}

fn wrap(text: &str, width: usize) -> String {
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return "Eroare de formatare".to_string();
    }
    let reference = parts[0];
    let reference_pad = if reference.len() < 6 { " ".repeat(6 - reference.len()) } else { "".to_string() };
    let options = Options::new(width - 6).break_words(false);
    let filled_text = fill(parts[1], &options);
    let mut result = String::new();
    for (i, line) in filled_text.lines().enumerate() {
        if i == 0 {
            result.push_str(&format!("{}{} ", reference, reference_pad));
        } else {
            result.push_str("       ");
        }
        result.push_str(line);
        if i < filled_text.lines().count() - 1 {
            result.push('\n');
        }
    }

    result
}

fn llm(api_key: &str, input: &str, model: &str) -> Result<String, Box<dyn Error>> {
    let client = Client::new();
    let system_prompt = "Esti un preot ortodox si imi raspunzi cu o singura referinta (unul sau mai multe versete consecutive) din biblia ortodoxa (atentie la psalmi!) Fc,Ies,Lv,Num,Dt,Ios,Jd,Rut,1Rg,2Rg,3Rg,4Rg,1Par,2Par,1Ezr,Ne,Est,Iov,Ps,Pr,Ecc,Cant,Is,Ir,Plg,Iz,Dn,Os,Am,Mi,Ioil,Avd,Ion,Naum,Avc,Sof,Ag,Za,Mal,Tob,Idt,Bar,Epist,Tin,3Ezr,Sol,Sir,Sus,Bel,1Mac,2Mac,3Mac,Man,Mt,Mc,Lc,In,FA,Rm,1Co,2Co,Ga,Ef,Flp,Col,1Tes,2Tes,1Tim,2Tim,Tit,Flm,Evr,Iac,1Ptr,2Ptr,1In,2In,3In,Iuda,Ap pe subiectul indicat. nu spui altceva inafara de referinta. formatul referintei este: Mt:10:20 sau Lc:20:2-3 sau Ap:1:2-4";
    let request_body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": input}
        ]
    });

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()?;

    let response_text = response.json::<serde_json::Value>()?;

    match response_text.get("error").and_then(|e| e.as_str()) {
        Some(error) => Err(error.into()),
        None => {
            let answer = response_text["choices"][0]["message"]["content"].as_str().ok_or("No content found")?;
            Ok(answer.to_string())
        }
    }
}

fn try_print_verses(bible: &Map<String, Value>, reference: &str, wrap_width: usize, from_llm: bool) {
    let pattern = Regex::new(r"(\w+):(\d+):(\d+)(?:-(\d+))?").expect("Invalid regex pattern");
    let mut book_name = "";
    let mut chapter_number = 0;
    let mut verse_range = (0, 0);

    if from_llm && pattern.is_match(reference) {
        let caps = pattern.captures(reference).unwrap(); // Since you checked with is_match, unwrap is safe here
        let parsed_ref = parse_reference(caps.get(0).unwrap().as_str(), true);
        book_name = parsed_ref.0;
        chapter_number = parsed_ref.1;
        verse_range = parsed_ref.2;
    } else if !from_llm {
        let parsed_ref = parse_reference(reference, false); // No check here? Tread carefully.
        book_name = parsed_ref.0;
        chapter_number = parsed_ref.1;
        verse_range = parsed_ref.2;
    }

    if !book_name.is_empty() {
        let found_book = find_book_name(bible, book_name);
        if let Some(verses) = get_verses(bible, &found_book, chapter_number, verse_range.0, verse_range.1) {
            println!("{}", found_book);
            for verse in verses {
                println!("{}", wrap(&verse, wrap_width));
            }
        } else {
            eprintln!("Nici un verset gasit pentru referinta specificata.");
        }
    } else {
        eprintln!("{}", reference);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let app = Command::new("biblia-cli-rs")
        .version("0.1")
        .author("TragDate <chiarel@tragdate.ninja>")
        .about("Citeste biblia in linia de comanda")
        .arg(Arg::new("reference").help("Referinta biblica in formatul Carte:Capitol:Verset sau Carte:Capitol:Verset-Verset").index(1))
        .arg(Arg::new("wrap").short('w').long("wrap").help("Nu mai pastreaza textul in limita de 80 de caractere pe linie"))
        .arg(
            Arg::new("llm")
                .short('l')
                .long("llm")
                .help("Cere un anumit subiect, iar AI-ul va raspunde cu o referinta biblica")
                .takes_value(true),
        )
        .get_matches();

    let api_key = dotenv::var("OPENAI_API_KEY").expect("Cheia de API nu e setata in varibila OPENAI_API_KEY.");
    let model = "gpt-4-1106-preview";
    let should_wrap = app.is_present("wrap");
    let wrap_width = if !should_wrap { 80 } else { std::usize::MAX };
    let bible_json = fs::read_to_string("/usr/local/share/biblia-cli-rs/biblia.json")?;
    let bible: Map<String, Value> = serde_json::from_str(&bible_json)?;

    if let Some(llm_value) = app.value_of("llm") {
        let reference = llm(&api_key.to_string(), llm_value, model).unwrap(); // again, be careful with unwrap
        try_print_verses(&bible, &reference, wrap_width, true);
    } else if let Some(reference) = app.value_of("reference") {
        try_print_verses(&bible, reference, wrap_width, false);
    }
    Ok(())
}
