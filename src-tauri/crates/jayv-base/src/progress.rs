//! O que o turno diz de si mesmo enquanto acontece.
//!
//! Antes deste módulo, um pedido sumia por dezenas de segundos: a tela mostrava
//! uma frase fixa e o trabalho corria no laço de fundo sem dar sinal. O `Beat` é
//! esse sinal, e o `Pulse` é o cano por onde ele sai — um cano que o núcleo
//! empurra sem saber quem está do outro lado. É de propósito: o mesmo
//! orquestrador atende a janela do Tauri e a linha de comando, e a linha de
//! comando não tem `AppHandle` nem banco para escrever.

use serde::Serialize;
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// O Jev trocou o modo do chat por conta própria: de onde ele saiu (`auto`
/// ou `plan`) e por quê (`asked`: o chat estava em planejamento e o pedido é
/// para implementar; `repeated`: no automático, o desenvolvedor pediu para
/// implementar de novo e o pedido anterior tinha ficado em planejamento).
#[derive(Debug,Clone,PartialEq,Eq,Serialize,serde::Deserialize)]
pub struct ModeSwitch { pub from:String, pub reason:String }

/// Uma parte de um pedido dividido entre agentes: o título e quem a faz.
#[derive(Debug,Clone,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SplitTask { pub title:String, pub provider:String, pub model:String }

/// Um sinal de vida do turno. O `Chunk` é a resposta crescendo; os outros são
/// etapas com nome próprio, e é por isso que cada um vira uma linha no log
/// enquanto o `Chunk` vira texto acumulado.
#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(tag="kind",rename_all="snake_case",rename_all_fields="camelCase")]
pub enum Beat {
    Gate{verdict:String,score:u8,demand:u8},
    Read{intent:String,complexity:String,source:String},
    Context{files:usize,tokens:usize},
    /// Quem atende: o agente (CLI), o modelo dele, o modo — `plan` ou
    /// `build` — e o papel que o Jev deu ao agente. `switched` vem quando o
    /// Jev tirou o chat do modo em que estava, para o balão dizer e desfazer.
    Route{provider:String,model:String,reason:String,mode:String,agent:Option<String>,#[serde(skip_serializing_if="Option::is_none")] switched:Option<ModeSwitch>},
    /// O agente não conseguiu começar e o pedido passou ao próximo: quem
    /// falhou e por quê (um aviso do i18n).
    Fallback{provider:String,error:String},
    /// Um agente de outro provedor está revisando o que o pedido mudou.
    Review{provider:String,model:String,files:usize},
    /// Um modelo de raciocínio está escrevendo o plano que o agente vai seguir.
    Plan{provider:String,model:String},
    /// O pedido foi dividido em partes que agentes fazem ao mesmo tempo.
    Split{tasks:Vec<SplitTask>},
    /// Uma das partes terminou: `applied` (as mudanças entraram no projeto),
    /// `empty` (não mudou nada), `conflict` (não encaixou; o patch ficou
    /// guardado em `patch`) ou `failed` (o agente falhou).
    Subtask{index:usize,title:String,provider:String,outcome:String,#[serde(skip_serializing_if="Option::is_none")] patch:Option<String>},
    Running,
    Agent{line:String},
    Chunk{text:String},
    Done{input_tokens:usize,output_tokens:usize,latency_ms:u128},
    Failed{error:String},
}

impl Beat {
    pub fn kind(&self)->&'static str {
        match self {
            Self::Gate{..}=>"gate", Self::Read{..}=>"read", Self::Context{..}=>"context",
            Self::Route{..}=>"route", Self::Fallback{..}=>"fallback", Self::Review{..}=>"review", Self::Plan{..}=>"plan", Self::Split{..}=>"split", Self::Subtask{..}=>"subtask", Self::Running=>"running", Self::Agent{..}=>"agent",
            Self::Chunk{..}=>"chunk", Self::Done{..}=>"done", Self::Failed{..}=>"failed",
        }
    }

    /// O corpo do evento, como vai para a coluna JSON. O `kind` aparece aqui e
    /// também na sua própria coluna: a coluna serve para filtrar sem abrir o
    /// JSON, e o JSON serve para a tela desenhar a linha sem um formato por
    /// tipo de evento.
    pub fn detail(&self)->Value { serde_json::to_value(self).unwrap_or(Value::Null) }

    /// O pedaço de resposta não é linha de log: ele se acumula em
    /// `turns.partial`, com folga, em vez de virar uma linha por token.
    pub fn is_chunk(&self)->bool { matches!(self,Self::Chunk{..}) }

    /// O último sinal do turno. Depois dele o texto acumulado tem de estar no
    /// disco, custe uma escrita extra.
    pub fn settles(&self)->bool { matches!(self,Self::Done{..}|Self::Failed{..}) }
}

/// O cano. `Pulse::silent()` existe para a linha de comando e para os testes:
/// sem ninguém escutando, o núcleo roda igual e não emite nada.
///
/// O canal é ilimitado de propósito. Um canal com teto faria o provedor parar
/// de ler a rede enquanto o disco não acompanhasse — o modelo esperaria pelo
/// SQLite. Os pedaços são pequenos, a consumidora grava com folga, e um envio
/// que nunca bloqueia é mais importante aqui do que um teto de memória.
#[derive(Clone)]
pub struct Pulse(Option<mpsc::UnboundedSender<Beat>>);

impl Pulse {
    pub fn channel()->(Self,mpsc::UnboundedReceiver<Beat>) {
        let (sender,receiver)=mpsc::unbounded_channel();
        (Self(Some(sender)),receiver)
    }

    pub fn silent()->Self { Self(None) }

    /// Engole erro de envio. Um canal fechado — a janela que sumiu, a
    /// consumidora que morreu — não pode derrubar o pedido que está sendo
    /// atendido; o turno vale mais que a narração dele.
    pub fn beat(&self,beat:Beat) { if let Some(sender)=&self.0 { let _=sender.send(beat); } }

    pub fn is_silent(&self)->bool { self.0.is_none() }
}

/// Quanto tempo e quantos bytes a resposta parcial espera antes de ir ao disco.
pub const FLUSH_AFTER:Duration=Duration::from_millis(400);
pub const FLUSH_BYTES:usize=2048;

/// A folga entre a tela e o disco. A webview recebe cada pedaço na hora — o IPC
/// do Tauri é barato e é isso que faz o texto crescer. O SQLite recebe o texto
/// acumulado a cada `FLUSH_AFTER` ou `FLUSH_BYTES`, o que vier primeiro: gravar
/// token a token seria um log de tokens, e não uma conversa guardada.
pub struct Debounce{written:Instant,waiting:usize}

impl Debounce {
    pub fn start(now:Instant)->Self { Self{written:now,waiting:0} }

    /// Contabiliza um pedaço e diz se é hora de gravar.
    pub fn accept(&mut self,bytes:usize,now:Instant)->bool {
        self.waiting=self.waiting.saturating_add(bytes);
        self.waiting>=FLUSH_BYTES || now.duration_since(self.written)>=FLUSH_AFTER
    }

    /// Chamado depois de gravar, e só então: a janela recomeça do momento da
    /// escrita, não do momento do pedaço.
    pub fn wrote(&mut self,now:Instant) { self.written=now; self.waiting=0; }

    pub fn waiting(&self)->usize { self.waiting }
}

/// De quanto em quanto tempo os pedaços da resposta vão juntos para a tela,
/// e quanto texto basta para ir antes disso.
pub const FRAME_EVERY:Duration=Duration::from_millis(50);
pub const FRAME_BYTES:usize=4096;

/// Os pedaços da resposta que esperam o próximo quadro da tela. O disco tem a
/// sua folga (`Debounce`); a tela tem esta, bem mais curta: o texto continua
/// crescendo enquanto chega, sem um aviso — e um redesenho — por token.
#[derive(Debug,Default)]
pub struct Frame { text:String, since:Option<tokio::time::Instant> }

impl Frame {
    /// Junta o pedaço. Devolve o quadro inteiro quando ele encheu e tem de ir já.
    pub fn push(&mut self,text:&str,now:tokio::time::Instant)->Option<String> {
        if self.since.is_none() { self.since=Some(now); }
        self.text.push_str(text);
        if self.text.len()>=FRAME_BYTES { self.take() } else { None }
    }

    /// Quando o quadro em espera tem de ir, se há um.
    pub fn due(&self)->Option<tokio::time::Instant> { self.since.map(|since|since+FRAME_EVERY) }

    /// O texto em espera, e o quadro recomeça vazio.
    pub fn take(&mut self)->Option<String> {
        self.since=None;
        (!self.text.is_empty()).then(||std::mem::take(&mut self.text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn a_silent_channel_does_not_bring_down_the_core() {
        let pulse=Pulse::silent();
        pulse.beat(Beat::Running);
        pulse.beat(Beat::Chunk{text:"nada escuta".into()});
        assert!(pulse.is_silent());
    }

    /// O canal fechado é o caso da janela que sumiu no meio do pedido. Ele não
    /// pode virar erro: o turno continua e termina.
    #[test] fn a_closed_channel_does_not_bring_down_the_core() {
        let (pulse,receiver)=Pulse::channel();
        drop(receiver);
        pulse.beat(Beat::Running);
    }

    #[test] fn chunks_arrive_in_order() {
        let (pulse,mut receiver)=Pulse::channel();
        pulse.beat(Beat::Chunk{text:"um ".into()});
        pulse.beat(Beat::Chunk{text:"dois".into()});
        pulse.beat(Beat::Done{input_tokens:10,output_tokens:2,latency_ms:5});
        let mut collected=String::new();
        let mut closed=false;
        while let Ok(beat)=receiver.try_recv() {
            match &beat { Beat::Chunk{text}=>collected.push_str(text), other=>closed=other.settles() }
        }
        assert_eq!(collected,"um dois");
        assert!(closed);
    }

    /// Cada etapa se descreve sozinha no JSON: a tela desenha a linha sem um
    /// formato por tipo de evento.
    #[test] fn the_detail_carries_its_own_kind() {
        let beat=Beat::Route{provider:"claude".into(),model:"claude-sonnet-4-5".into(),reason:"melhor pontuação".into(),mode:"plan".into(),agent:Some("reviewer".into()),switched:None};
        let detail=beat.detail();
        assert_eq!(detail["kind"],"route");
        assert_eq!(detail["model"],"claude-sonnet-4-5");
        assert_eq!(detail["mode"],"plan");
        assert_eq!(detail["agent"],"reviewer");
        assert_eq!(beat.kind(),"route");
        assert!(detail.get("switched").is_none(),"sem troca, o campo nem aparece");
        let switched=Beat::Route{provider:"claude".into(),model:"sonnet".into(),reason:String::new(),mode:"build".into(),agent:None,switched:Some(ModeSwitch{from:"plan".into(),reason:"asked".into()})}.detail();
        assert_eq!(switched["switched"]["from"],"plan");
    }

    /// Muitos pedaços pequenos e nenhum tempo decorrido dão uma escrita só. Era
    /// isto que faltava para o texto parcial não virar um log de tokens.
    /// Os tokens de um quadro vão juntos, na ordem; o quadro cheio vai na hora.
    #[test] fn pieces_wait_for_the_frame_and_keep_their_order() {
        let now=tokio::time::Instant::now();
        let mut frame=Frame::default();
        assert_eq!(frame.due(),None,"sem texto, nada a esperar");
        assert_eq!(frame.push("um ",now),None);
        assert_eq!(frame.push("dois",now+Duration::from_millis(10)),None);
        assert_eq!(frame.due(),Some(now+FRAME_EVERY),"o prazo conta do primeiro pedaço");
        assert_eq!(frame.take().as_deref(),Some("um dois"));
        assert_eq!(frame.take(),None);
        assert_eq!(frame.push(&"x".repeat(FRAME_BYTES),now).map(|text|text.len()),Some(FRAME_BYTES),"quadro cheio vai já");
        assert_eq!(frame.due(),None);
    }

    #[test] fn many_small_chunks_make_a_single_write() {
        let now=Instant::now();
        let mut slack=Debounce::start(now);
        let mut writes=0;
        for _ in 0..200 { if slack.accept(4,now) { writes+=1; slack.wrote(now); } }
        assert_eq!(writes,200*4/FLUSH_BYTES);
    }

    #[test] fn time_alone_forces_a_write() {
        let now=Instant::now();
        let mut slack=Debounce::start(now);
        assert!(!slack.accept(1,now),"um byte não justifica ir ao disco");
        assert!(slack.accept(1,now+FLUSH_AFTER),"a janela venceu e o texto tem de ser gravado");
    }

    #[test] fn volume_alone_forces_a_write() {
        let now=Instant::now();
        let mut slack=Debounce::start(now);
        assert!(slack.accept(FLUSH_BYTES,now),"o texto acumulado passou do teto");
    }

    /// A janela recomeça na escrita. Sem isto, o primeiro estouro deixaria toda
    /// gravação seguinte vencida e a folga viraria enfeite.
    #[test] fn the_window_restarts_on_write() {
        let start=Instant::now();
        let mut slack=Debounce::start(start);
        let due=start+FLUSH_AFTER;
        assert!(slack.accept(1,due));
        slack.wrote(due);
        assert!(!slack.accept(1,due),"a folga voltou a valer depois de gravar");
        assert_eq!(slack.waiting(),1);
    }

    #[test] fn only_the_end_of_the_turn_asks_for_a_flush() {
        assert!(Beat::Done{input_tokens:0,output_tokens:0,latency_ms:0}.settles());
        assert!(Beat::Failed{error:"caiu".into()}.settles());
        assert!(!Beat::Running.settles());
        assert!(Beat::Chunk{text:"x".into()}.is_chunk());
        assert!(!Beat::Running.is_chunk());
    }
}
