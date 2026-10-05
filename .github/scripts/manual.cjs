// A documentação do JayV para o site: cada funcionalidade é um JSON em
// `docs/manual/features/<id>.json`, os comandos ficam em
// `docs/manual/commands.json` e a ordem das páginas em `docs/manual/index.json`.
// O `releases-docs.yml` publica tudo, junto do changelog inteiro, no
// repositório público de releases; o site lê de lá pela função `releases` do
// Supabase, nunca direto do GitHub.
//
// O texto vai em inglês. O site traduz pelas chaves `docs.<id>.title`,
// `.summary`, `.usage` e `docs.command.<id>`, que ganham migração nos dez
// idiomas no JayV-Coder/supabase, como toda chave nova.
//
//   node .github/scripts/manual.cjs check          confere a documentação
//   node .github/scripts/manual.cjs build <pasta>  grava o que vai ser publicado

const fs = require("node:fs");
const path = require("node:path");
const { allReleases } = require("./whats-new.cjs");

const ID = /^[A-Za-z][A-Za-z0-9]{0,63}$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const KINDS = ["chat", "shortcut", "cli"];
const TEXTS = ["title", "summary", "usage"];

const read = (root, file) => fs.readFileSync(path.join(root, file), "utf8");
const json = (root, file) => JSON.parse(read(root, file));

/** As chaves do catálogo dos planos (`src/modules/plans/catalog.ts`). */
function planKeys(root) {
  const source = read(root, "src/modules/plans/catalog.ts");
  const list = source.match(/FEATURES[^=]*=\s*\[([\s\S]*?)\]/)?.[1] ?? "";
  return [...list.matchAll(/"([A-Za-z]+)"/g)].map(([, key]) => key);
}

/** A documentação como está no repositório, sem conferir nada. */
function load(root = process.cwd()) {
  const index = json(root, "docs/manual/index.json");
  const files = fs.readdirSync(path.join(root, "docs/manual/features")).filter((name) => name.endsWith(".json")).sort();
  const features = files.map((name) => ({ file: name, value: json(root, `docs/manual/features/${name}`) }));
  const { commands } = json(root, "docs/manual/commands.json");
  return { index, features, commands };
}

/** Tudo que está errado na documentação; vazio quando está pronta para sair. */
function problems(root = process.cwd()) {
  const found = [];
  const { index, features, commands } = load(root);
  const version = json(root, "src-tauri/tauri.conf.json").version;
  const categories = Array.isArray(index.categories) ? index.categories : [];
  const listed = Array.isArray(index.features) ? index.features : [];
  const commandIds = new Set();

  for (const command of commands ?? []) {
    if (!ID.test(command.id ?? "")) found.push(`command with an invalid id: ${JSON.stringify(command.id)}`);
    if (commandIds.has(command.id)) found.push(`command ${command.id} appears twice`);
    commandIds.add(command.id);
    if (!KINDS.includes(command.kind)) found.push(`command ${command.id}: kind must be one of ${KINDS.join(", ")}`);
    if (typeof command.usage !== "string" || !command.usage.trim()) found.push(`command ${command.id}: missing usage`);
    if (typeof command.detail !== "string" || !command.detail.trim()) found.push(`command ${command.id}: missing detail`);
  }

  const ids = new Set();
  for (const { file, value } of features) {
    const id = value.id;
    if (`${id}.json` !== file) found.push(`${file}: the id must match the file name`);
    if (!ID.test(id ?? "")) found.push(`${file}: invalid id`);
    ids.add(id);
    if (!categories.includes(value.category)) found.push(`${file}: unknown category ${JSON.stringify(value.category)}`);
    if (value.since !== null && !(VERSION.test(value.since ?? "") )) found.push(`${file}: since must be a version or null`);
    if (value.since && compare(value.since, version) > 0) found.push(`${file}: since ${value.since} is newer than the app (${version})`);
    if (value.plan !== null && !planKeys(root).includes(value.plan)) found.push(`${file}: plan ${JSON.stringify(value.plan)} is not in the plans catalog`);
    if (!Array.isArray(value.commands)) found.push(`${file}: commands must be a list`);
    for (const command of value.commands ?? []) if (!commandIds.has(command)) found.push(`${file}: unknown command ${command}`);
    for (const text of TEXTS) if (typeof value[text] !== "string" || !value[text].trim()) found.push(`${file}: missing ${text}`);
  }

  for (const id of listed) if (!ids.has(id)) found.push(`index.json lists ${id}, which has no file`);
  for (const id of ids) if (!listed.includes(id)) found.push(`features/${id}.json is not in index.json`);
  if (new Set(listed).size !== listed.length) found.push("index.json lists a feature twice");
  // Todo recurso que um plano liga ou desliga tem a sua página.
  const documented = new Set(features.map(({ value }) => value.plan).filter(Boolean));
  for (const key of planKeys(root)) if (!documented.has(key)) found.push(`the plan feature ${key} has no page in docs/manual/features`);
  return found;
}

function compare(a, b) {
  const [left, right] = [a, b].map((version) => version.split(".").map(Number));
  for (let index = 0; index < 3; index++) if (left[index] !== right[index]) return Math.sign(left[index] - right[index]);
  return 0;
}

/** O que vai para o repositório de releases: caminho → conteúdo. */
function publication(root = process.cwd(), now = new Date()) {
  const { index, features, commands } = load(root);
  const version = json(root, "src-tauri/tauri.conf.json").version;
  const byId = new Map(features.map(({ value }) => [value.id, value]));
  const files = {
    "manual/index.json": { version, updatedAt: now.toISOString(), categories: index.categories, features: index.features },
    "manual/commands.json": { commands },
    "changelog.json": { version, releases: allReleases(read(root, "src/modules/changelog/releases.ts"), read(root, "src/modules/i18n/messages/en.ts")) },
  };
  for (const id of index.features) files[`manual/features/${id}.json`] = byId.get(id);
  return Object.fromEntries(Object.entries(files).map(([file, value]) => [file, `${JSON.stringify(value, null, 2)}\n`]));
}

module.exports = { load, problems, publication };

if (require.main === module) {
  const [command, out] = process.argv.slice(2);
  const found = problems();
  if (found.length) {
    console.error(found.map((line) => `- ${line}`).join("\n"));
    process.exit(1);
  }
  if (command === "build") {
    for (const [file, content] of Object.entries(publication())) {
      const target = path.join(out ?? "manual-out", file);
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, content);
    }
  }
  console.log("docs/manual: ok");
}
