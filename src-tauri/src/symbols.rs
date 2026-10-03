//! O índice de símbolos do repositório: onde cada função, tipo e classe é
//! definida e quais arquivos usam o que os outros definem.
//!
//! É lido do código pelo tree-sitter, na máquina, sem modelo nenhum: zero
//! token. Serve a três coisas — as definições do mapa que vai aos agentes
//! (do arquivo inteiro, não só do trecho que a busca escolheu), o peso de um
//! nome definido na busca e as ligações entre arquivos ("usa" / "usado por").
//! Só entra aresta lida na árvore: um nome definido em muitos arquivos é
//! ambíguo e não liga nada, em vez de ligar errado.

use crate::rag::IndexedFile;
use serde::Serialize;
use std::{collections::{BTreeSet, HashMap, HashSet}, sync::OnceLock};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor};

/// Um nome definido em mais arquivos do que isto (`new`, `get`, `render`) não
/// diz de qual deles se fala: não vira aresta.
const AMBIGUOUS_DEFINITIONS:usize=3;
/// Nomes mais curtos que isto são ruído (`f`, `id`, `ok`).
const SHORT_NAME:usize=3;
/// Quanto de uma linha de definição vai como assinatura.
const SIGNATURE_CHARS:usize=140;
/// Os nós de acesso a membro das gramáticas (Rust, JS/TS, Python, Go).
const MEMBER_ACCESS:[&str;4]=["field_expression","member_expression","attribute","selector_expression"];
/// Arquivos maiores que isto não são analisados: quase sempre é código gerado.
const PARSE_LIMIT:usize=400_000;

#[derive(Debug,Clone,PartialEq,Serialize)]
pub struct Definition { pub name:String, pub kind:String, pub line:usize, pub signature:String }

#[derive(Debug,Clone,Default)]
pub struct FileSymbols { pub definitions:Vec<Definition>, pub references:HashSet<String> }

#[derive(Clone,Copy,PartialEq,Eq,Hash)]
enum Grammar { Rust, Python, JavaScript, TypeScript, Tsx, Go }

/// Definições que as consultas de tags dos próprios pacotes deixam de fora.
const RUST_EXTRA:&str="(const_item name: (identifier) @name) @definition.constant\n(static_item name: (identifier) @name) @definition.constant";
const TYPESCRIPT_EXTRA:&str="(type_alias_declaration name: (type_identifier) @name) @definition.type\n(enum_declaration name: (identifier) @name) @definition.enum\n(class_declaration name: (type_identifier) @name) @definition.class";

impl Grammar {
    fn of(path:&str)->Option<Self> {
        match path.rsplit_once('.')?.1.to_lowercase().as_str() {
            "rs"=>Some(Self::Rust), "py"=>Some(Self::Python), "js"|"mjs"|"cjs"|"jsx"=>Some(Self::JavaScript),
            "ts"|"mts"|"cts"=>Some(Self::TypeScript), "tsx"=>Some(Self::Tsx), "go"=>Some(Self::Go), _=>None,
        }
    }

    /// Quem pode chamar quem: TypeScript e JavaScript se importam; um nome
    /// igual num arquivo de outra linguagem é coincidência, não ligação.
    fn family(self)->u8 { match self { Self::Rust=>0, Self::Python=>1, Self::JavaScript|Self::TypeScript|Self::Tsx=>2, Self::Go=>3 } }

    fn language(self)->Language {
        match self {
            Self::Rust=>tree_sitter_rust::LANGUAGE.into(), Self::Python=>tree_sitter_python::LANGUAGE.into(),
            Self::JavaScript=>tree_sitter_javascript::LANGUAGE.into(), Self::TypeScript=>tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx=>tree_sitter_typescript::LANGUAGE_TSX.into(), Self::Go=>tree_sitter_go::LANGUAGE.into(),
        }
    }

    fn tags(self)->String {
        match self {
            Self::Rust=>format!("{}\n{RUST_EXTRA}",tree_sitter_rust::TAGS_QUERY),
            Self::Python=>tree_sitter_python::TAGS_QUERY.into(),
            Self::JavaScript=>tree_sitter_javascript::TAGS_QUERY.into(),
            // O TypeScript estende o JavaScript: as tags dele só têm o que é dele.
            Self::TypeScript|Self::Tsx=>format!("{}\n{}\n{TYPESCRIPT_EXTRA}",tree_sitter_javascript::TAGS_QUERY,tree_sitter_typescript::TAGS_QUERY),
            Self::Go=>tree_sitter_go::TAGS_QUERY.into(),
        }
    }

    /// A consulta compilada, uma vez por linguagem. Uma consulta que não
    /// compila (pacote trocado) desliga só aquela linguagem.
    fn query(self)->Option<&'static Query> {
        static QUERIES:OnceLock<HashMap<Grammar,Option<Query>>>=OnceLock::new();
        QUERIES.get_or_init(||{
            [Self::Rust,Self::Python,Self::JavaScript,Self::TypeScript,Self::Tsx,Self::Go].into_iter().map(|grammar|{
                let query=Query::new(&grammar.language(),&grammar.tags()).map_err(|error|eprintln!("símbolos: consulta de tags inválida ({error})")).ok();
                (grammar,query)
            }).collect()
        }).get(&self).and_then(Option::as_ref)
    }
}

/// Se o índice sabe ler este arquivo.
pub fn supported(path:&str)->bool { Grammar::of(path).is_some() }

/// As definições e os nomes usados num arquivo. `None` quando a linguagem não
/// tem gramática aqui ou o arquivo é grande demais.
pub fn extract(path:&str,content:&str)->Option<FileSymbols> {
    let grammar=Grammar::of(path)?;
    if content.len()>PARSE_LIMIT { return None; }
    let query=grammar.query()?;
    let mut parser=Parser::new();
    parser.set_language(&grammar.language()).ok()?;
    let tree=parser.parse(content,None)?;
    let names=query.capture_names();
    let mut symbols=FileSymbols::default();
    let mut seen=HashSet::new();
    let mut cursor=QueryCursor::new();
    let mut matches=cursor.matches(query,tree.root_node(),content.as_bytes());
    while let Some(found)=matches.next() {
        let named=found.captures().iter().find(|capture|names[capture.index as usize]=="name");
        let name=named.and_then(|capture|capture.node.utf8_text(content.as_bytes()).ok());
        // `x.len()`, `store.save()`: o nome do método não diz de que tipo ele
        // é, e ligaria o arquivo a qualquer um que defina um `len`.
        let method_call=named.and_then(|capture|capture.node.parent()).is_some_and(|parent|MEMBER_ACCESS.contains(&parent.kind()));
        let role=found.captures().iter().find_map(|capture|{ let label=names[capture.index as usize]; (label.starts_with("definition.")||label.starts_with("reference.")).then_some((label,capture.node)) });
        let (Some(name),Some((label,node)))=(name,role) else { continue };
        if let Some(kind)=label.strip_prefix("definition.") {
            let line=node.start_position().row;
            if !seen.insert((name.to_string(),line)) { continue; }
            symbols.definitions.push(Definition{name:name.into(),kind:kind.into(),line:line+1,signature:signature(content,line)});
        } else if name.chars().count()>=SHORT_NAME&&!method_call {
            symbols.references.insert(name.to_string());
        }
    }
    symbols.definitions.sort_by_key(|definition|definition.line);
    Some(symbols)
}

/// A linha onde a definição começa, sem o corpo.
fn signature(content:&str,row:usize)->String {
    let line=content.lines().nth(row).unwrap_or_default().trim();
    let line=line.split_once(" {").map_or(line,|(head,_)|head).trim_end_matches(['{',' ',':','=']);
    if line.chars().count()>SIGNATURE_CHARS { format!("{}…",line.chars().take(SIGNATURE_CHARS).collect::<String>()) } else { line.to_string() }
}

/// Os símbolos de todos os arquivos lidos e as ligações entre eles.
#[derive(Debug,Default)]
pub struct SymbolIndex {
    files:HashMap<String,(String,FileSymbols)>,
    /// Palavras dos nomes definidos em cada arquivo, para a busca.
    terms:HashMap<String,HashSet<String>>,
    defined_in:HashMap<String,Vec<String>>,
    uses:HashMap<String,BTreeSet<String>>,
    used_by:HashMap<String,BTreeSet<String>>,
}

impl SymbolIndex {
    /// Lê de novo só os arquivos cujo hash mudou e refaz as ligações.
    pub fn update(&mut self,files:&[IndexedFile]) {
        let mut kept=std::mem::take(&mut self.files);
        for file in files.iter().filter(|file|supported(&file.path)) {
            let reused=kept.remove(&file.path).filter(|(hash,_)|*hash==file.hash);
            let entry=match reused { Some(entry)=>Some(entry), None=>extract(&file.path,&file.content).map(|symbols|(file.hash.clone(),symbols)) };
            if let Some(entry)=entry { self.files.insert(file.path.clone(),entry); }
        }
        self.link();
    }

    fn link(&mut self) {
        self.defined_in.clear(); self.uses.clear(); self.used_by.clear(); self.terms.clear();
        for (path,(_,symbols)) in &self.files {
            let mut names=symbols.definitions.iter().map(|definition|definition.name.as_str()).collect::<Vec<_>>();
            names.sort(); names.dedup();
            for name in names { self.defined_in.entry(name.to_string()).or_default().push(path.clone()); }
            self.terms.insert(path.clone(),symbols.definitions.iter().flat_map(|definition|crate::rag::terms(&definition.name)).collect());
        }
        for paths in self.defined_in.values_mut() { paths.sort(); }
        for (path,(_,symbols)) in &self.files {
            for name in &symbols.references {
                let Some(owners)=self.defined_in.get(name).filter(|owners|owners.len()<=AMBIGUOUS_DEFINITIONS) else { continue };
                let family=Grammar::of(path).map(Grammar::family);
                for owner in owners.iter().filter(|owner|*owner!=path&&Grammar::of(owner).map(Grammar::family)==family) {
                    self.uses.entry(path.clone()).or_default().insert(owner.clone());
                    self.used_by.entry(owner.clone()).or_default().insert(path.clone());
                }
            }
        }
    }

    pub fn len(&self)->usize { self.files.len() }
    pub fn is_empty(&self)->bool { self.files.is_empty() }
    pub fn definitions(&self,path:&str)->Option<&[Definition]> { self.files.get(path).map(|(_,symbols)|symbols.definitions.as_slice()) }
    /// Os nomes que mais dizem do arquivo: tipos primeiro, depois funções;
    /// constantes e módulos por último.
    pub fn headline(&self,path:&str,limit:usize)->Vec<&str> {
        let rank=|kind:&str|match kind { "class"|"interface"|"type"|"enum"=>0, "function"|"method"|"macro"=>1, _=>2 };
        let mut definitions=self.definitions(path).unwrap_or_default().iter().collect::<Vec<_>>();
        definitions.sort_by_key(|definition|(rank(&definition.kind),definition.line));
        let mut names=Vec::new();
        for definition in definitions {
            if names.len()==limit { break; }
            if !names.contains(&definition.name.as_str()) { names.push(definition.name.as_str()); }
        }
        names
    }
    pub fn terms(&self,path:&str)->Option<&HashSet<String>> { self.terms.get(path) }
    /// Os arquivos que definem o que este usa.
    pub fn uses(&self,path:&str)->Vec<&str> { self.uses.get(path).map(|set|set.iter().map(String::as_str).collect()).unwrap_or_default() }
    /// Os arquivos que usam o que este define.
    pub fn used_by(&self,path:&str)->Vec<&str> { self.used_by.get(path).map(|set|set.iter().map(String::as_str).collect()).unwrap_or_default() }

    /// Onde um nome é definido, com a linha e a assinatura.
    pub fn find(&self,name:&str)->Vec<(&str,&Definition)> {
        let mut found=self.defined_in.get(name).into_iter().flatten().filter_map(|path|self.files.get(path).map(|(_,symbols)|(path.as_str(),symbols))).flat_map(|(path,symbols)|symbols.definitions.iter().filter(|definition|definition.name==name).map(move|definition|(path,definition))).collect::<Vec<_>>();
        found.sort_by(|left,right|left.0.cmp(right.0).then(left.1.line.cmp(&right.1.line)));
        found
    }

    /// Os arquivos que citam um nome.
    pub fn references(&self,name:&str)->Vec<&str> {
        let mut found=self.files.iter().filter(|(_,(_,symbols))|symbols.references.contains(name)).map(|(path,_)|path.as_str()).collect::<Vec<_>>();
        found.sort();
        found
    }

    /// Os arquivos de que mais gente depende: os pontos centrais do projeto.
    pub fn hubs(&self,limit:usize)->Vec<(&str,usize)> {
        let mut hubs=self.used_by.iter().map(|(path,users)|(path.as_str(),users.len())).collect::<Vec<_>>();
        hubs.sort_by(|left,right|right.1.cmp(&left.1).then(left.0.cmp(right.0)));
        hubs.truncate(limit);
        hubs
    }

    /// Quem é afetado ao mudar um arquivo: quem o usa, e quem usa esses, até
    /// `depth` passos.
    pub fn impact(&self,path:&str,depth:usize)->Vec<String> {
        let mut reached=BTreeSet::new();
        let mut frontier=vec![path.to_string()];
        for _ in 0..depth {
            let next=frontier.iter().flat_map(|current|self.used_by(current)).filter(|user|*user!=path&&!reached.contains(*user)).map(str::to_string).collect::<BTreeSet<_>>();
            if next.is_empty() { break; }
            reached.extend(next.iter().cloned());
            frontier=next.into_iter().collect();
        }
        reached.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path:&str,content:&str)->IndexedFile { IndexedFile{path:path.into(),language:String::new(),hash:format!("{:x}",content.len()),content:content.into(),tokens:HashSet::new()} }

    #[test]
    fn definitions_come_from_the_whole_file_in_every_supported_language() {
        let filler="// filler\n".repeat(2_000);
        let rust=extract("src/lib.rs",&format!("pub struct Cache {{ size: usize }}\n{filler}impl Cache {{\n    pub fn invalidate(&mut self) {{ clear(); }}\n}}\nconst LIMIT: usize = 3;\n")).expect("rust");
        let names=rust.definitions.iter().map(|definition|definition.name.as_str()).collect::<Vec<_>>();
        assert_eq!(names,["Cache","invalidate","LIMIT"],"o que fica depois dos 12 mil caracteres também entra");
        assert!(rust.definitions[1].signature.starts_with("pub fn invalidate(&mut self)"));
        assert!(rust.references.contains("clear"));
        let typescript=extract("web/src/main.tsx","export function App() { return render(); }\nexport type Props = { id: string };\ninterface Store { get(): void }\nexport const useThing = () => fetchThing();\n").expect("tsx");
        let names=typescript.definitions.iter().map(|definition|definition.name.as_str()).collect::<Vec<_>>();
        for expected in ["App","Props","Store","useThing"] { assert!(names.contains(&expected),"falta {expected} em {names:?}"); }
        assert!(typescript.references.contains("render")&&typescript.references.contains("fetchThing"));
        let python=extract("util.py","class Store:\n    def load(self):\n        return parse()\n").expect("python");
        assert_eq!(python.definitions.iter().map(|definition|definition.name.as_str()).collect::<Vec<_>>(),["Store","load"]);
        let go=extract("main.go","package main\nfunc Run() { Start() }\ntype Server struct{}\n").expect("go");
        assert!(go.definitions.iter().any(|definition|definition.name=="Run")&&go.definitions.iter().any(|definition|definition.name=="Server"));
        assert!(extract("notes.md","# fn not code").is_none());
    }

    #[test]
    fn files_are_linked_by_what_they_use_and_ambiguous_names_link_nothing() {
        let mut index=SymbolIndex::default();
        let files=[
            file("src/cache.rs","pub fn invalidate_entries() {}\npub fn new() {}\n"),
            file("src/orchestrator.rs","fn run() { invalidate_entries(); new(); }\npub fn new() {}\n"),
            file("src/queue.rs","fn attend() { run(); }\npub fn new() {}\n"),
            file("src/other.rs","pub fn new() {}\n"),
        ];
        index.update(&files);
        assert_eq!(index.uses("src/orchestrator.rs"),["src/cache.rs"],"`new` está em quatro arquivos e não liga ninguém");
        assert_eq!(index.used_by("src/cache.rs"),["src/orchestrator.rs"]);
        assert_eq!(index.impact("src/cache.rs",2),["src/orchestrator.rs","src/queue.rs"]);
        assert_eq!(index.find("invalidate_entries")[0].0,"src/cache.rs");
        assert_eq!(index.references("run"),["src/queue.rs"]);
        assert_eq!(index.hubs(1),[("src/cache.rs",1)]);
        assert!(index.terms("src/cache.rs").expect("termos").contains("invalidate"));
        index.update(&[file("src/store.rs","pub struct Store;\npub const LIMIT: u8 = 1;\npub fn save_all() {}\n"),file("src/user.rs","fn go(store: Store) { store.save_all(); }\n")]);
        assert!(index.uses("src/user.rs").is_empty(),"chamada de método não liga arquivos");
        assert_eq!(index.headline("src/store.rs",2),["Store","save_all"],"tipos e funções antes de constantes");
        index.update(&[file("src/store.rs","pub fn save_all() {}\n"),file("web/save.ts","saveAll(); save_all();\n"),file("web/api.ts","export function saveAll() {}\n")]);
        assert_eq!(index.uses("web/save.ts"),["web/api.ts"],"o mesmo nome em Rust não liga um arquivo TypeScript");
    }

    #[test]
    fn only_changed_files_are_parsed_again_and_removed_ones_leave() {
        let mut index=SymbolIndex::default();
        index.update(&[file("a.rs","fn first() {}"),file("b.rs","fn second() {}")]);
        assert_eq!(index.len(),2);
        index.update(&[file("a.rs","fn renamed_one() {}")]);
        assert_eq!(index.len(),1);
        assert!(index.find("first").is_empty()&&index.find("second").is_empty());
        assert_eq!(index.find("renamed_one").len(),1);
    }
}
