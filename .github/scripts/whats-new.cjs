// As novidades da versão que vai ser publicada, para o release levar junto:
// o aplicativo instalado lê esse trecho no `latest.json` e mostra na janela
// "Novidades" o que a versão nova traz antes de a pessoa atualizar.
//
// Lê a primeira entrada de `src/modules/changelog/releases.ts` e os textos em
// inglês de `en.ts` (os outros idiomas o app já recebe do Supabase). Vai no
// corpo do release como comentário HTML com JSON em base64, para não aparecer
// na página do GitHub. Roda no `release.yml` e também à mão:
//
//   node .github/scripts/whats-new.cjs

const fs = require("node:fs");
const path = require("node:path");

const MARK = "whats-new";

/** O texto de uma chave em `en.ts`, com os escapes de string do JS desfeitos. */
function english(source, key) {
  const match = source.match(new RegExp(`"${key.replace(/\./g, "\\.")}":\\s*"((?:\\\\.|[^"\\\\])*)"`));
  return match ? JSON.parse(`"${match[1]}"`) : null;
}

/** A versão mais nova do changelog, com o inglês de cada item. */
function latestRelease(releasesSource, enSource) {
  const start = releasesSource.indexOf("export const RELEASES");
  const head = releasesSource.slice(start).match(/version:\s*"([^"]+)",\s*date:\s*"([^"]+)",\s*items:\s*\[([\s\S]*?)\]/);
  if (!head) return null;
  const items = [...head[3].matchAll(/kind:\s*"(feature|fix)",\s*id:\s*"([^"]+)"/g)].map(([, kind, id]) => ({
    kind,
    id,
    title: english(enSource, `whatsNew.item.${id}.title`),
    detail: english(enSource, `whatsNew.item.${id}.detail`),
  }));
  return { version: head[1], date: head[2], items };
}

/** O comentário que vai no fim do corpo do release. */
function whatsNewComment(root = process.cwd()) {
  const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
  const release = latestRelease(read("src/modules/changelog/releases.ts"), read("src/modules/i18n/messages/en.ts"));
  if (!release) return "";
  return `<!-- ${MARK}: ${Buffer.from(JSON.stringify(release), "utf8").toString("base64")} -->`;
}

module.exports = { latestRelease, whatsNewComment };

if (require.main === module) {
  const comment = whatsNewComment();
  console.log(comment);
  const encoded = comment.match(/: (\S+) -->/)?.[1];
  if (encoded) console.log(JSON.stringify(JSON.parse(Buffer.from(encoded, "base64").toString("utf8")), null, 2));
}
