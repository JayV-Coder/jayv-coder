//! Imprime o seed de `jev_questions` e `jev_parameters` a partir das perguntas
//! que o Rust faz hoje:
//!
//!   cargo run --example jev_seed > ../supabase/migrations/20260930120200_seed_jev.sql

use jayv_lib::{asking, gatekeeper, jev};
use serde_json::Value;

fn literal(text:&str)->String { format!("'{}'",text.replace('\'',"''")) }

/// Dólar-citado para o JSON entrar sem escape nenhum.
fn json(value:&Value)->String {
    let text=value.to_string();
    assert!(!text.contains("$json$"),"pergunta com $json$: {text}");
    format!("$json${text}$json$::jsonb")
}

fn main() {
    let sets=[
        ("entry",gatekeeper::entry_questions()),
        ("routing",jev::routing_questions()),
        ("verification",jev::verification_questions()),
        ("asking",asking::questions()),
    ];
    println!("-- Gerado por src-tauri/examples/jev_seed.rs. Não edite à mão.\n");
    println!("insert into public.jev_questions (question_set, id, body, position) values");
    let rows:Vec<String>=sets.iter().flat_map(|(set,questions)|questions.iter().enumerate().map(move |(position,(id,question))|
        format!("  ({}, {}, {}, {position})",literal(set),literal(id),json(&serde_json::to_value(question).expect("pergunta"))))).collect();
    println!("{}",rows.join(",\n"));
    println!("on conflict (question_set, id) do update set body = excluded.body, position = excluded.position;\n");
    println!("insert into public.jev_parameters (key, value) values");
    let rows:Vec<String>=gatekeeper::parameters().iter().map(|(key,value)|format!("  ({}, {})",literal(key),json(value))).collect();
    println!("{}",rows.join(",\n"));
    println!("on conflict (key) do update set value = excluded.value;");
}
