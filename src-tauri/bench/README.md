# jayv bench

Mede o JayV pelo que ele promete a quem está começando: **quantos pedidos e
quantos dólares até a tarefa ficar pronta**, com o JayV e com o agente
sozinho.

```sh
jayv bench src-tauri/bench/tasks.yaml
```

## Como funciona

- Cada tarefa tem o roteiro de um iniciante (`script`) e um comando que diz
  se ela ficou pronta (`verify`, saída 0 = pronta). Os dois lados mandam um
  pedido por volta até o comando passar ou acabarem as voltas
  (`max_rounds`, padrão 5). Acabando o roteiro, o último pedido se repete.
- Cada lado roda numa cópia própria do projeto (`git worktree` quando a pasta
  é um repositório; cópia da pasta quando não é). O projeto original não é
  tocado.
- A ordem alterna: na primeira tarefa o JayV roda primeiro, na segunda o
  agente direto, e assim por diante.
- O lado do JayV passa pela portaria como no app (Jev quando há sessão,
  heurística local quando não): a continuação herda o veredito, o pedido
  barrado gasta a volta sem chamar agente, e o "perguntar" é confirmado com
  o pedido reescrito (coluna `asked`, confirmados/barrados).
- O lado direto usa o agente e o modelo que um pedido de código médio
  receberia, em build, do primeiro ao último pedido, com a sessão dele.
- O veredito sai do custo em dólar que os agentes informam. Quando alguma
  chamada não informa, a comparação usa tokens ponderados (o cache lido
  pesa 0,1 e o escrito 1,25). O que o Jev gastou aparece à parte: ele é pago
  pelo JayV, não pela cota de quem usa.

## Formato

```yaml
repository: fixture        # relativo a este arquivo; sem ele, o --root
tasks:
  - id: slugify
    verify: python3 -m unittest -q tests.test_text
    max_rounds: 5
    script:
      - "cria uma função pra transformar texto em slug"
      - "não funcionou, o teste ainda falha"
```

`fixture/` é um exemplo neutro em Python, sem dependências, com dez tarefas
cujos testes falham hoje. Para medir no seu projeto, escreva um arquivo com as
suas tarefas e aponte `repository` para ele.

## O que conferir

Rodar duas vezes seguidas e comparar: o custo total deve variar menos de 15%
entre as execuções antes de tirar conclusão de uma mudança no JayV.
