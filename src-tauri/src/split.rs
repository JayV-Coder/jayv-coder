//! Planejar com um, construir com outro: num pedido complexo do modo build, um
//! modelo forte de raciocínio lê o pedido em somente leitura e escreve o
//! plano; o agente que constrói recebe o plano junto com o pedido. Os dois
//! podem ser de provedores diferentes.

/// As complexidades em que vale pagar a ida a mais ao planejador.
pub fn wants_plan(complexity:&str)->bool { complexity=="complex" }

/// O que o planejador recebe: o pedido e o pedido de não mexer em nada.
pub fn prompt(request:&str,builder:&str)->String {
    format!("Another coding agent ({builder}) will implement the request below right after you. Your job is only the plan: read what you need from the project, but do not edit any file.

<request>
{request}
</request>

Write a numbered implementation plan: the files to change or create, what changes in each, the order, the edge cases to handle and how to check that it works (commands to run, tests to add). Be concrete and short; no code blocks longer than a few lines.")
}

/// O plano como vai ao agente que constrói, junto do pedido.
pub fn handoff(plan:&str,planner:&str)->Option<String> {
    let plan=plan.trim();
    (!plan.is_empty()).then(||format!("An implementation plan for this request, written by {planner} after reading the project. Follow it unless you find it wrong, and say so if you deviate:\n<plan>\n{plan}\n</plan>"))
}

#[cfg(test)] mod tests {
    use super::*;

    #[test] fn only_complex_requests_get_a_separate_plan() {
        assert!(wants_plan("complex"));
        assert!(!wants_plan("medium")&&!wants_plan("simple"));
    }

    #[test] fn an_empty_plan_is_not_handed_over() {
        assert!(handoff("  \n","Claude Code").is_none());
        let note=handoff("1. mude o roteador","Claude Code").expect("plano");
        assert!(note.contains("Claude Code")&&note.contains("1. mude o roteador"));
    }
}
