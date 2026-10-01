// Monta o README.md do repositório público de releases a partir dos releases
// publicados: a versão mais nova separada por sistema e, abaixo, todas as
// versões. Roda no Actions (`releases-readme.yml`) e também à mão:
//
//   node .github/scripts/releases-readme.cjs < releases.json
//
// lendo a resposta de `GET /repos/{owner}/{repo}/releases`.

/** Os instaladores que importam para quem baixa. Assinaturas, o `latest.json`
 * e os pacotes `.app.tar.gz` existem para a atualização automática. */
const KINDS = [
  { system: "Windows", label: "Instalador (.exe)", test: (name) => /-setup\.exe$/i.test(name) },
  { system: "Windows", label: "MSI (.msi)", test: (name) => /\.msi$/i.test(name) },
  { system: "macOS", label: "Apple Silicon (.dmg)", test: (name) => /_aarch64\.dmg$/i.test(name) },
  { system: "macOS", label: "Intel (.dmg)", test: (name) => /_x64\.dmg$/i.test(name) },
  { system: "Linux", label: "AppImage", test: (name) => /\.AppImage$/i.test(name) },
  { system: "Linux", label: "Debian/Ubuntu (.deb)", test: (name) => /\.deb$/i.test(name) },
  { system: "Linux", label: "Fedora/RHEL (.rpm)", test: (name) => /\.rpm$/i.test(name) },
];
const SYSTEMS = ["Windows", "macOS", "Linux"];

const size = (bytes) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;
const day = (iso) => (iso ? iso.slice(0, 10) : "—");

function installers(release) {
  return KINDS.flatMap((kind) => release.assets.filter((asset) => kind.test(asset.name)).map((asset) => ({ ...kind, asset })));
}

/** Os releases que contam: publicados, com instalador, do mais novo para o
 * mais antigo. */
function published(releases) {
  return releases
    .filter((release) => !release.draft && installers(release).length > 0)
    .sort((a, b) => new Date(b.published_at) - new Date(a.published_at));
}

function render(releases) {
  const list = published(releases);
  const lines = [
    "# JayV",
    "",
    "Instaladores do JayV para Windows, macOS e Linux. Quem já tem o JayV instalado recebe as versões novas pela atualização automática do próprio aplicativo.",
    "",
    "",
  ];
  if (list.length === 0) return [...lines, "Nenhum release publicado ainda.", ""].join("\n");

  const [latest] = list;
  lines.push(`## Versão mais recente: ${latest.tag_name}`, "", `Publicada em ${day(latest.published_at)} · [notas da versão](${latest.html_url})`, "");
  for (const system of SYSTEMS) {
    const found = installers(latest).filter((item) => item.system === system);
    if (found.length === 0) continue;
    lines.push(`### ${system}`, "", "| Pacote | Arquivo | Tamanho |", "| --- | --- | --- |");
    for (const { label, asset } of found) lines.push(`| ${label} | [${asset.name}](${asset.browser_download_url}) | ${size(asset.size)} |`);
    lines.push("");
  }

  lines.push("## Todas as versões", "", `| Versão | Data | ${SYSTEMS.join(" | ")} |`, `| --- | --- | ${SYSTEMS.map(() => "---").join(" | ")} |`);
  for (const release of list) {
    const cells = SYSTEMS.map((system) => {
      const found = installers(release).filter((item) => item.system === system);
      return found.length ? found.map(({ label, asset }) => `[${label}](${asset.browser_download_url})`).join("<br>") : "—";
    });
    lines.push(`| [${release.tag_name}](${release.html_url}) | ${day(release.published_at)} | ${cells.join(" | ")} |`);
  }
  lines.push("");
  return lines.join("\n");
}

module.exports = { render };

if (require.main === module) {
  let input = "";
  process.stdin.on("data", (chunk) => { input += chunk; });
  process.stdin.on("end", () => process.stdout.write(render(JSON.parse(input))));
}
