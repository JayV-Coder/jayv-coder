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

/// Um sinal de vida do turno. O `Chunk` é a resposta crescendo; os outros são
/// etapas com nome próprio, e é por isso que cada um vira uma linha no log
/// enquanto o `Chunk` vira texto acumulado.
#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(tag="kind",rename_all="snake_case",rename_all_fields="camelCase")]
pub enum Beat {
    Gate{verdict:String,score:u8,demand:u8},
    Read{intent:String,complexity:String,source:String},
    Context{files:usize,tokens:usize},
    Route{provider:String,model:String,reason:String},
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
            Self::Route{..}=>"route", Self::Running=>"running", Self::Agent{..}=>"agent",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn o_canal_mudo_nao_derruba_o_nucleo() {
        let pulse=Pulse::silent();
        pulse.beat(Beat::Running);
        pulse.beat(Beat::Chunk{text:"nada escuta".into()});
        assert!(pulse.is_silent());
    }

    /// O canal fechado é o caso da janela que sumiu no meio do pedido. Ele não
    /// pode virar erro: o turno continua e termina.
    #[test] fn o_canal_fechado_nao_derruba_o_nucleo() {
        let (pulse,receiver)=Pulse::channel();
        drop(receiver);
        pulse.beat(Beat::Running);
    }

    #[test] fn os_pedacos_chegam_na_ordem() {
        let (pulse,mut receiver)=Pulse::channel();
        pulse.beat(Beat::Chunk{text:"um ".into()});
        pulse.beat(Beat::Chunk{text:"dois".into()});
        pulse.beat(Beat::Done{input_tokens:10,output_tokens:2,latency_ms:5});
        let mut texto=String::new();
        let mut fechou=false;
        while let Ok(beat)=receiver.try_recv() {
            match &beat { Beat::Chunk{text}=>texto.push_str(text), other=>fechou=other.settles() }
        }
        assert_eq!(texto,"um dois");
        assert!(fechou);
    }

    /// Cada etapa se descreve sozinha no JSON: a tela desenha a linha sem um
    /// formato por tipo de evento.
    #[test] fn o_detalhe_carrega_o_proprio_tipo() {
        let beat=Beat::Route{provider:"claude".into(),model:"claude-sonnet-4-5".into(),reason:"melhor pontuação".into()};
        let detail=beat.detail();
        assert_eq!(detail["kind"],"route");
        assert_eq!(detail["model"],"claude-sonnet-4-5");
        assert_eq!(beat.kind(),"route");
    }

    /// Muitos pedaços pequenos e nenhum tempo decorrido dão uma escrita só. Era
    /// isto que faltava para o texto parcial não virar um log de tokens.
    #[test] fn muitos_pedacos_pequenos_dao_uma_escrita_so() {
        let agora=Instant::now();
        let mut folga=Debounce::start(agora);
        let mut escritas=0;
        for _ in 0..200 { if folga.accept(4,agora) { escritas+=1; folga.wrote(agora); } }
        assert_eq!(escritas,200*4/FLUSH_BYTES);
    }

    #[test] fn o_tempo_sozinho_forca_a_escrita() {
        let agora=Instant::now();
        let mut folga=Debounce::start(agora);
        assert!(!folga.accept(1,agora),"um byte não justifica ir ao disco");
        assert!(folga.accept(1,agora+FLUSH_AFTER),"a janela venceu e o texto tem de ser gravado");
    }

    #[test] fn o_volume_sozinho_forca_a_escrita() {
        let agora=Instant::now();
        let mut folga=Debounce::start(agora);
        assert!(folga.accept(FLUSH_BYTES,agora),"o texto acumulado passou do teto");
    }

    /// A janela recomeça na escrita. Sem isto, o primeiro estouro deixaria toda
    /// gravação seguinte vencida e a folga viraria enfeite.
    #[test] fn a_janela_recomeca_na_escrita() {
        let inicio=Instant::now();
        let mut folga=Debounce::start(inicio);
        let vencida=inicio+FLUSH_AFTER;
        assert!(folga.accept(1,vencida));
        folga.wrote(vencida);
        assert!(!folga.accept(1,vencida),"a folga voltou a valer depois de gravar");
        assert_eq!(folga.waiting(),1);
    }

    #[test] fn so_o_fim_do_turno_pede_descarga() {
        assert!(Beat::Done{input_tokens:0,output_tokens:0,latency_ms:0}.settles());
        assert!(Beat::Failed{error:"caiu".into()}.settles());
        assert!(!Beat::Running.settles());
        assert!(Beat::Chunk{text:"x".into()}.is_chunk());
        assert!(!Beat::Running.is_chunk());
    }
}
