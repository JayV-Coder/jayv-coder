import { TALLY, useGate } from "@/modules/gate";
import { useT } from "@/modules/i18n";
import { FolderIcon, LogoIcon, PathText } from "@/components/atoms";
import { TallyItem } from "@/components/molecules";

/** O placar da portaria: de qual projeto ela cuida e quantos passaram, viraram
 * pergunta, foram barrados ou segurados. */
export function GateBoard() {
  const t = useT();
  const { project, feed } = useGate();
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-7 gap-y-5 border-b border-rail-2 bg-[linear-gradient(#0f1216,#0a0d10)] px-7 py-[17px]">
      <div className="flex min-w-0 items-center gap-[13px]">
        <LogoIcon className="size-8 flex-none text-[#89939d] [&_.logo-halo]:stroke-ask [&_.logo-lamp]:fill-ask" />
        <div className="flex min-w-0 flex-col">
          <strong className="text-[22px] font-semibold tracking-[0.05em] text-[#eef1f4]">{t("nav.gate")}</strong>
          <span className="flex min-w-0 items-center gap-2 text-[13.5px] text-dim">
            {!project ? t("gate.subtitle") : (
              <>
                <b className="font-semibold text-[#cdd4db]">{project.name}</b>
                {project.rootPath
                  ? <><FolderIcon className="size-[15px] flex-none" /><PathText title={project.rootPath} className="max-w-[38ch] font-gate-mono text-[12.5px] text-faint">{project.rootPath}</PathText></>
                  : <i className="text-faint">{t("common.noFolder")}</i>}
              </>
            )}
          </span>
        </div>
      </div>
      <div className="flex">
        {TALLY.map(([key, aspect, label]) => <TallyItem key={key} aspect={aspect} count={feed.tally[key] ?? 0} label={t(label)} />)}
      </div>
    </div>
  );
}
