//! A segunda opinião: depois que um agente muda o projeto no modo build, um
//! agente de outro provedor lê o diff do pedido, em somente leitura, e aponta
//! o que está errado antes de a resposta ficar pronta. Os dois nunca escrevem
//! no mesmo arquivo: quem revisa só lê.

use crate::{live_files::{ChangeKind, Session}, model::ModelSelection};
use similar::TextDiff;

/// O teto do diff mandado ao revisor, em caracteres (uns 15 mil tokens). O
/// que passa disso é cortado arquivo a arquivo, e o revisor é avisado.
const MAX_DIFF:usize=60_000;
/// As linhas de contexto em volta de cada trecho alterado.
const CONTEXT_LINES:usize=3;

/// O diff unificado do que o pedido mudou, arquivo a arquivo, na ordem em que
/// mudaram. Um arquivo coberto pela privacidade aparece só pelo nome; um
/// binário ou grande demais, pelo nome e pelo motivo. Sem mudança nenhuma,
/// nada: não há o que revisar.
pub fn diff(session:&Session)->Option<String> {
    let mut changes=session.changes().to_vec();
    if changes.is_empty() { return None; }
    changes.reverse();
    let mut out=String::new();
    let mut cut=0;
    for change in &changes {
        let Some(view)=session.view(&change.path) else { continue };
        let piece=if view.hidden { format!("--- {0}\n+++ {0}\n(private file: content not shown)\n",view.path) }
            else if view.binary || view.too_large { format!("--- {0}\n+++ {0}\n({1})\n",view.path,if view.binary {"binary file"} else {"file too large to show"}) }
            else {
                let before=view.before.clone().unwrap_or_default();
                let after=if change.kind==ChangeKind::Removed { String::new() } else { view.after.clone().unwrap_or_default() };
                let (old,new)=match change.kind { ChangeKind::Created=>("/dev/null".to_string(),format!("b/{}",view.path)), ChangeKind::Removed=>(format!("a/{}",view.path),"/dev/null".to_string()), _=>(format!("a/{}",view.path),format!("b/{}",view.path)) };
                let unknown=if view.before_known { "" } else { "(the previous content is unknown: the whole file is shown as new)\n" };
                format!("{unknown}{}",TextDiff::from_lines(&before,&after).unified_diff().context_radius(CONTEXT_LINES).header(&old,&new))
            };
        if out.len()+piece.len()>MAX_DIFF { cut+=1; continue; }
        out.push_str(&piece);
        if !out.ends_with('\n') { out.push('\n'); }
    }
    if cut>0 { out.push_str(&format!("\n({cut} more changed file(s) left out to keep the review short)\n")); }
    (!out.trim().is_empty()).then_some(out)
}

/// Quem revisa: o melhor candidato para uma revisão que seja de outro
/// provedor que o executor e que possa rodar em somente leitura. Sem outro
/// provedor, ninguém revisa: o mesmo agente revendo o próprio trabalho não é
/// segunda opinião.
pub fn pick<'a>(ranked:&'a [ModelSelection],executor:&str,read_only:impl Fn(&str)->bool)->Option<&'a ModelSelection> {
    ranked.iter().find(|candidate|candidate.provider!=executor&&read_only(&candidate.provider))
}

/// O pedido ao revisor. Ele escreve a seção inteira, com o título, no idioma
/// da resposta: assim o texto entra no fim da resposta como está.
pub fn prompt(request:&str,diff:&str,executor:&str,reviewer:&str)->String {
    format!("You are {reviewer}, reviewing in read-only mode a change that another coding agent ({executor}) just made to this project. Do not edit any file.

The developer's request:
<request>
{request}
</request>

The change, as a unified diff:
<diff>
{diff}
</diff>

Point out what would make this change wrong: bugs, missing cases the request asked for, code that will not build, broken tests, security problems. Ignore style and naming. Start with a level-3 Markdown heading that means \"Review by {reviewer}\" in the reply language. Then list at most 8 problems, most severe first, each with the file and line and one sentence on why it is wrong. If nothing needs fixing, write only the heading and one sentence saying so.")
}

/// O texto da revisão como entra no fim da resposta: separado do resto.
pub fn section(review:&str)->Option<String> {
    let review=review.trim();
    (!review.is_empty()).then(||format!("\n\n---\n\n{review}\n"))
}

#[cfg(test)] mod tests {
    use super::*;
    use crate::firewall::ContextFirewall;
    use std::process::Command;

    fn git(dir:&std::path::Path,args:&[&str]) { assert!(Command::new("git").current_dir(dir).args(args).output().expect("git").status.success(),"git {args:?}"); }

    fn repository()->tempfile::TempDir {
        let dir=tempfile::tempdir().expect("pasta");
        git(dir.path(),&["init","-q"]);
        git(dir.path(),&["config","user.email","t@t"]);
        git(dir.path(),&["config","user.name","t"]);
        std::fs::write(dir.path().join("lib.rs"),"fn one() {}\nfn two() {}\n").expect("lib");
        std::fs::write(dir.path().join(".env"),"KEY=1\n").expect("env");
        git(dir.path(),&["add","."]);
        git(dir.path(),&["commit","-qm","base"]);
        dir
    }

    #[test] fn the_diff_shows_what_the_request_changed_and_hides_private_files() {
        let dir=repository();
        let privacy=crate::config::PrivacyConfig{deny:vec![".env".into()],..Default::default()};
        let mut session=Session::start("t",dir.path(),ContextFirewall::new(privacy));
        std::fs::write(dir.path().join("lib.rs"),"fn one() {}\nfn three() {}\n").expect("muda");
        std::fs::write(dir.path().join("new.rs"),"fn new() {}\n").expect("cria");
        std::fs::write(dir.path().join(".env"),"KEY=2\n").expect("segredo");
        session.poll();
        let text=diff(&session).expect("diff");
        assert!(text.contains("-fn two() {}")&&text.contains("+fn three() {}"),"{text}");
        assert!(text.contains("--- /dev/null")&&text.contains("+fn new() {}"),"{text}");
        assert!(text.contains("private file")&&!text.contains("KEY=2"),"o segredo não vai ao revisor: {text}");
    }

    #[test] fn nothing_changed_means_nothing_to_review() {
        let dir=repository();
        let mut session=Session::start("t",dir.path(),ContextFirewall::new(Default::default()));
        session.poll();
        assert!(diff(&session).is_none());
    }

    #[test] fn the_reviewer_is_another_provider_that_can_read_only() {
        let candidate=|provider:&str|ModelSelection{provider:provider.into(),model_name:"m".into(),..Default::default()};
        let ranked=[candidate("claude"),candidate("codex"),candidate("cursor")];
        assert_eq!(pick(&ranked,"claude",|_|true).map(|chosen|chosen.provider.as_str()),Some("codex"));
        assert_eq!(pick(&ranked,"claude",|provider|provider=="cursor").map(|chosen|chosen.provider.as_str()),Some("cursor"));
        assert!(pick(&ranked[..1],"claude",|_|true).is_none(),"sozinho, o agente não revisa a si mesmo");
    }
}
