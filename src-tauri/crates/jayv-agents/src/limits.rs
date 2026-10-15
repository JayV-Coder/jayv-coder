//! O limite de uso esgotado: reconhecer que um agente parou por isso e quando o
//! limite volta.
//!
//! Os agentes rodam sem terminal, então o único aviso do prazo é o texto do
//! erro, que cada CLI escreve do seu jeito:
//! - Claude Code: `Claude AI usage limit reached|1760000000` (instante em
//!   segundos), ou `You've hit your limit · resets 3pm (America/Sao_Paulo)`;
//! - Codex: `You've hit your usage limit ... try again at 3:45 PM` ou
//!   `try again in 2 hours 15 minutes`;
//! - Copilot, Cursor e as APIs: `Retry-After: 120`, `retry after 30 seconds`,
//!   `resets at 2026-10-14T18:00:00Z`.
//!
//! Hora sem data vale para a próxima vez que o relógio a marcar, no fuso do
//! computador (o fuso escrito entre parênteses não é lido: o agente e o app
//! rodam na mesma máquina).

use crate::guard;
use chrono::{DateTime, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use regex::Regex;
use std::sync::OnceLock;

/// O limite que um agente atingiu e, quando o texto disse, quando ele volta.
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct LimitHit { pub agent:String, pub resets_at:Option<DateTime<Utc>> }

/// O maior prazo que se espera de pé: a janela da semana. Além disso o texto
/// foi mal lido, e a espera cai no padrão.
const LONGEST_WAIT_DAYS:i64=8;

fn pattern(cell:&'static OnceLock<Regex>,source:&str)->&'static Regex { cell.get_or_init(||Regex::new(source).expect("limit regex")) }

/// Se o texto é o aviso de limite de uso esgotado. Login, chave e crédito
/// acabado não entram: esperar não resolve nenhum dos três.
pub fn is_limit(text:&str)->bool {
    static LIMIT:OnceLock<Regex>=OnceLock::new();
    static NOT_LIMIT:OnceLock<Regex>=OnceLock::new();
    let limit=pattern(&LIMIT,r"(?i)usage limit|rate.?limit|hit your (?:\w+ )?limit|limit (?:reached|exceeded|has been reached)|reached (?:your|the|its) [\w\- ]{0,30}limit|(?:quota|usage|requests?) (?:exceeded|exhausted|used up)|exceeded your (?:current )?(?:quota|usage|limit)|too many requests|\b429\b|limit will reset|resets? (?:at|in|on) ");
    let denied=pattern(&NOT_LIMIT,r"(?i)not logged in|please log ?in|sign ?in|unauthori[sz]ed|invalid api key|authentication (?:failed|error)|insufficient (?:credits?|funds)");
    limit.is_match(text)&&!denied.is_match(text)
}

/// Quando o limite volta, segundo o texto do agente. `zone` é o fuso do
/// computador. Vazio quando o texto não diz, ou diz algo fora do possível.
pub fn reset_time(text:&str,now:DateTime<Utc>,zone:FixedOffset)->Option<DateTime<Utc>> {
    let found=epoch(text,now).or_else(||relative(text,now)).or_else(||iso(text,zone)).or_else(||clock(text,now,zone))?;
    // Um instante que já passou vale como agora: o limite acabou de voltar.
    let found=found.max(now);
    (found-now<=Duration::days(LONGEST_WAIT_DAYS)).then_some(found)
}

/// O instante em segundos (ou milissegundos) depois de uma barra, como o do
/// aviso do Claude: `usage limit reached|1760000000`.
fn epoch(text:&str,now:DateTime<Utc>)->Option<DateTime<Utc>> {
    static EPOCH:OnceLock<Regex>=OnceLock::new();
    let digits=pattern(&EPOCH,r"(?i)limit[^|\n]{0,40}\|\s*(\d{10,13})\b").captures(text)?[1].parse::<i64>().ok()?;
    let seconds=if digits>99_999_999_999 { digits/1000 } else { digits };
    let at=DateTime::from_timestamp(seconds,0)?;
    (at>now-Duration::days(1)).then_some(at)
}

/// Uma duração dita por extenso: `in 2 hours 15 minutes`, `retry after 90
/// seconds`, `Retry-After: 120`.
fn relative(text:&str,now:DateTime<Utc>)->Option<DateTime<Utc>> {
    static AFTER:OnceLock<Regex>=OnceLock::new();
    static IN:OnceLock<Regex>=OnceLock::new();
    static UNIT:OnceLock<Regex>=OnceLock::new();
    if let Some(found)=pattern(&AFTER,r"(?i)retry[- ]after:?\s*(\d+)\b(?:\s*(ms|milliseconds?))?").captures(text) {
        let amount=found[1].parse::<i64>().ok()?;
        return Some(now+if found.get(2).is_some() { Duration::milliseconds(amount) } else { Duration::seconds(amount) });
    }
    let tail=pattern(&IN,r"(?i)(?:try again|retry|resets?|available again|come back|wait)\s+(?:in|after|for)\s+((?:about |around |~)?\d+(?:\.\d+)?\s*(?:days?|d|hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)\b(?:[\s,]*(?:and\s+)?\d+(?:\.\d+)?\s*(?:days?|d|hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)\b)*)").captures(text)?[1].to_string();
    let mut total=0.0_f64;
    for part in pattern(&UNIT,r"(?i)(\d+(?:\.\d+)?)\s*(days?|d|hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)\b").captures_iter(&tail) {
        let amount=part[1].parse::<f64>().ok()?;
        let unit=part[2].to_lowercase();
        total+=amount*match unit.chars().next()? { 'd'=>86_400.0, 'h'=>3_600.0, 'm'=>60.0, _=>1.0 };
    }
    (total>0.0).then(||now+Duration::seconds(total.ceil() as i64))
}

/// Uma data e hora completas, `2026-10-14T18:00:00Z`; sem fuso, vale o do
/// computador.
fn iso(text:&str,zone:FixedOffset)->Option<DateTime<Utc>> {
    static STAMP:OnceLock<Regex>=OnceLock::new();
    let found=pattern(&STAMP,r"(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2}(?::\d{2})?)(?:\.\d+)?\s*(Z|[+-]\d{2}:?\d{2})?").captures(text)?;
    let time=if found[2].len()==5 { format!("{}:00",&found[2]) } else { found[2].to_string() };
    let naive=NaiveDateTime::parse_from_str(&format!("{} {}",&found[1],time),"%Y-%m-%d %H:%M:%S").ok()?;
    match found.get(3).map(|zone|zone.as_str()) {
        Some("Z")=>Some(Utc.from_utc_datetime(&naive)),
        Some(offset)=>{
            let sign=if offset.starts_with('-') { -1 } else { 1 };
            let digits:String=offset.chars().filter(char::is_ascii_digit).collect();
            let seconds=sign*(digits.get(0..2)?.parse::<i32>().ok()?*3600+digits.get(2..4)?.parse::<i32>().ok()?*60);
            FixedOffset::east_opt(seconds)?.from_local_datetime(&naive).single().map(|at|at.with_timezone(&Utc))
        }
        None=>local(zone,naive),
    }
}

fn local(zone:FixedOffset,naive:NaiveDateTime)->Option<DateTime<Utc>> {
    match zone.from_local_datetime(&naive) { LocalResult::Single(at)=>Some(at.with_timezone(&Utc)), _=>None }
}

const MONTHS:[&str;12]=["jan","feb","mar","apr","may","jun","jul","aug","sep","oct","nov","dec"];

/// Uma hora do relógio, com ou sem dia: `resets 3pm`, `try again at 3:45 PM`,
/// `resets Oct 20 at 9am`, `resets at 15:30`. Sem dia, é a próxima vez que o
/// relógio marcar essa hora.
fn clock(text:&str,now:DateTime<Utc>,zone:FixedOffset)->Option<DateTime<Utc>> {
    static CLOCK:OnceLock<Regex>=OnceLock::new();
    let found=pattern(&CLOCK,r"(?i)(?:resets?|try again|available again|until|back)\s*(?:at|on|by|around)?\s*(?:(?:(?P<month>jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s+(?P<day>\d{1,2})(?:st|nd|rd|th)?,?(?:\s+\d{4})?)\s*(?:,|at|·|-)?\s*)?(?P<hour>\d{1,2})(?::(?P<minute>\d{2}))?\s*(?P<meridiem>am|pm)?\b").captures(text)?;
    let mut hour:u32=found.name("hour")?.as_str().parse().ok()?;
    let minute:u32=found.name("minute").map_or(Some(0),|minute|minute.as_str().parse().ok())?;
    let meridiem=found.name("meridiem").map(|word|word.as_str().to_lowercase());
    // Um número solto ("resets 5") não é hora: precisa de am/pm ou de minutos.
    if meridiem.is_none()&&found.name("minute").is_none() { return None; }
    match meridiem.as_deref() {
        Some(word)=>{ if !(1..=12).contains(&hour) { return None; } hour%=12; if word=="pm" { hour+=12; } }
        None=>{ if hour>23 { return None; } }
    }
    let time=NaiveTime::from_hms_opt(hour,minute.min(59),0)?;
    let today=now.with_timezone(&zone).date_naive();
    if let (Some(month),Some(day))=(found.name("month"),found.name("day")) {
        let month=MONTHS.iter().position(|name|**name==month.as_str().to_lowercase()[..3])? as u32+1;
        let day:u32=day.as_str().parse().ok()?;
        let mut at=local(zone,NaiveDate::from_ymd_opt(chrono::Datelike::year(&today),month,day)?.and_time(time))?;
        // O dia que já passou neste ano é o do ano que vem.
        if at<now-Duration::days(1) { at=local(zone,NaiveDate::from_ymd_opt(chrono::Datelike::year(&today)+1,month,day)?.and_time(time))?; }
        return Some(at);
    }
    let mut at=local(zone,today.and_time(time))?;
    if at<=now { at=local(zone,today.succ_opt()?.and_time(time))?; }
    Some(at)
}

/// O limite que está por trás de um erro de agente: o texto do motivo diz que
/// o uso acabou, ou a trava de consumo recusou abrir o agente. Vazio para
/// qualquer outra falha.
pub fn from_error(error:&anyhow::Error,agent:&str,now:DateTime<Utc>,zone:FixedOffset)->Option<LimitHit> {
    use crate::i18n::{Param,Text};
    for cause in error.chain() {
        if let Some(text)=cause.downcast_ref::<Text>() {
            if text.key=="guard.quota" { return Some(LimitHit{agent:agent.to_string(),resets_at:guard::reset_of(agent)}); }
            if text.key=="provider.failed" {
                if let Some(Param::Plain(reason))=text.params.get("reason") {
                    if is_limit(reason) { return Some(LimitHit{agent:agent.to_string(),resets_at:reset_time(reason,now,zone)}); }
                }
            }
        }
    }
    let text=format!("{error:#}");
    is_limit(&text).then(||LimitHit{agent:agent.to_string(),resets_at:reset_time(&text,now,zone)})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text:&str)->DateTime<Utc> { DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc) }
    fn brazil()->FixedOffset { FixedOffset::west_opt(3*3600).unwrap() }
    fn read(text:&str)->Option<DateTime<Utc>> { reset_time(text,at("2026-10-14T12:00:00Z"),brazil()) }

    #[test] fn usage_limit_messages_are_recognized_and_other_refusals_are_not() {
        assert!(is_limit("Claude AI usage limit reached|1792000000"));
        assert!(is_limit("You've hit your limit · resets 3pm (America/Sao_Paulo)"));
        assert!(is_limit("You've hit your usage limit. Try again at 3:45 PM."));
        assert!(is_limit("HTTP 429 Too Many Requests"));
        assert!(!is_limit("Not logged in · Please run /login"));
        assert!(!is_limit("Invalid API key"));
        assert!(!is_limit("There is an issue with the selected model"));
    }

    #[test] fn claude_reports_the_reset_as_an_epoch() {
        assert_eq!(read("Claude AI usage limit reached|1792000000"),DateTime::from_timestamp(1_792_000_000,0));
        assert_eq!(read("Claude AI usage limit reached|1792000000000"),DateTime::from_timestamp(1_792_000_000,0),"milissegundos");
    }

    #[test] fn a_clock_time_without_a_day_is_the_next_time_it_strikes() {
        // 12:00 UTC é 09:00 em Brasília: 15h ainda é hoje, 8h só amanhã.
        assert_eq!(read("You've hit your limit · resets 3pm (America/Sao_Paulo)"),Some(at("2026-10-14T18:00:00Z")));
        assert_eq!(read("usage limit reached, resets at 8:00 AM"),Some(at("2026-10-15T11:00:00Z")));
        assert_eq!(read("You've hit your usage limit. Try again at 3:45 PM."),Some(at("2026-10-14T18:45:00Z")));
        assert_eq!(read("rate limit, resets at 15:30"),Some(at("2026-10-14T18:30:00Z")));
        assert_eq!(read("limit reached, resets 5"),None,"número solto não é hora");
    }

    #[test] fn a_clock_time_with_a_day_is_that_day() {
        assert_eq!(read("usage limit reached · resets Oct 20 at 9am"),Some(at("2026-10-20T12:00:00Z")));
        assert_eq!(read("usage limit reached · resets Oct 20, 9am"),Some(at("2026-10-20T12:00:00Z")));
    }

    #[test] fn durations_are_added_to_now() {
        assert_eq!(read("usage limit reached, try again in 2 hours 15 minutes"),Some(at("2026-10-14T14:15:00Z")));
        assert_eq!(read("rate limit exceeded. Retry-After: 120"),Some(at("2026-10-14T12:02:00Z")));
        assert_eq!(read("429 retry after 30 seconds"),Some(at("2026-10-14T12:00:30Z")));
        assert_eq!(read("limit hit; resets in 3 days and 4 hours"),Some(at("2026-10-17T16:00:00Z")));
    }

    #[test] fn full_timestamps_keep_their_zone() {
        assert_eq!(read("quota exceeded, resets at 2026-10-14T18:00:00Z"),Some(at("2026-10-14T18:00:00Z")));
        assert_eq!(read("quota exceeded, resets at 2026-10-14T18:00:00-03:00"),Some(at("2026-10-14T21:00:00Z")));
        assert_eq!(read("quota exceeded, resets at 2026-10-14 18:00"),Some(at("2026-10-14T21:00:00Z")),"sem fuso vale o do computador");
    }

    #[test] fn impossible_or_missing_times_are_not_guessed() {
        assert_eq!(read("usage limit reached"),None);
        assert_eq!(read("usage limit reached, try again in 400 days"),None);
        assert_eq!(read("usage limit reached, resets at 2020-01-01T00:00:00Z"),Some(at("2026-10-14T12:00:00Z")),"o que já passou vale agora");
    }

    #[test] fn an_agent_error_carries_its_limit() {
        use crate::i18n::Text;
        let now=at("2026-10-14T12:00:00Z");
        let refused:anyhow::Error=Text::new("provider.failed").with("provider","claude").with("exit","code: 1").with("reason","Claude AI usage limit reached|1792000000").with("origin","announced").into();
        assert_eq!(from_error(&refused,"claude",now,brazil()),Some(LimitHit{agent:"claude".into(),resets_at:DateTime::from_timestamp(1_792_000_000,0)}));
        let login:anyhow::Error=Text::new("provider.failed").with("provider","claude").with("exit","code: 1").with("reason","Not logged in").with("origin","announced").into();
        assert_eq!(from_error(&login,"claude",now,brazil()),None);
        let unrelated=anyhow::anyhow!("disk is full");
        assert_eq!(from_error(&unrelated,"codex",now,brazil()),None);
    }
}
