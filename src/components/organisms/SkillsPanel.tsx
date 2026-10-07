import { useEffect, useRef, useState } from "react";
import { DownloadIcon, FolderPlusIcon, SearchIcon, Trash2Icon } from "lucide-react";
import { useT } from "@/modules/i18n";
import { useIntentHandler } from "@/modules/commands";
import { useFeature } from "@/modules/plans";
import { installFromSkillHub, installSkillFromFolder, installSkillFromText, loadSkills, removeSkill, searchSkillHub, setSkillEnabled, useSkills } from "@/modules/skills";
import { LoadingNote } from "@/components/atoms";
import { SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { OrgExtensionsSection } from "./OrgExtensionsSection";

/** Configurações › Skills: as skills (pastas com `SKILL.md`) que o Jev pode
 * escolher para cada pedido. Grava na hora, sem o Salvar da página. */
export function SkillsPanel() {
  const t = useT();
  const skills = useSkills((state) => state.skills);
  const installing = useSkills((state) => state.installing);
  const hits = useSkills((state) => state.hits);
  const searching = useSkills((state) => state.searching);
  const downloading = useSkills((state) => state.downloading);
  const [pasted, setPasted] = useState("");
  const [query, setQuery] = useState("");
  const searchBox = useRef<HTMLInputElement>(null);
  const hubAllowed = useFeature("skillsHub");
  useEffect(() => { void loadSkills(); }, []);
  useIntentHandler("installSkill", () => void installSkillFromFolder());
  useIntentHandler("searchSkillHub", () => searchBox.current?.focus());
  if (skills === null) return <LoadingNote>{t("settings.loading")}</LoadingNote>;

  return (
    <div className="grid gap-5">
      <SettingsSection
        title={t("skills.title")}
        description={t("skills.description")}
        action={<Button type="button" size="sm" variant="outline" disabled={installing} onClick={() => void installSkillFromFolder()}><FolderPlusIcon aria-hidden="true" />{t("skills.install")}</Button>}
      >
        {skills.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("skills.empty")}</p>
        ) : (
          <ul className="grid gap-2">
            {skills.map((skill) => (
              <li key={skill.name} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2">
                <Switch checked={skill.enabled} aria-label={t("skills.enabled", { name: skill.name })} onCheckedChange={(enabled) => void setSkillEnabled(skill.name, enabled)} />
                <div className="grid min-w-0 flex-1 gap-0.5">
                  <span className="text-sm font-medium">{skill.name}</span>
                  <span className="line-clamp-2 text-xs text-muted-foreground">{skill.description}</span>
                </div>
                <Button type="button" variant="ghost" size="icon-sm" aria-label={t("skills.remove", { name: skill.name })} title={t("skills.remove", { name: skill.name })} onClick={() => void removeSkill(skill.name)}><Trash2Icon aria-hidden="true" /></Button>
              </li>
            ))}
          </ul>
        )}
        <p className="text-xs leading-snug text-muted-foreground">{t("skills.notes")}</p>
      </SettingsSection>
      {hubAllowed && <SettingsSection title={t("skills.hub.title")} description={t("skills.hub.description")}>
        <form className="flex gap-2" onSubmit={(event) => { event.preventDefault(); void searchSkillHub(query); }}>
          <Input ref={searchBox} value={query} placeholder={t("skills.hub.placeholder")} aria-label={t("skills.hub.placeholder")} onChange={(event) => setQuery(event.target.value)} />
          <Button type="submit" variant="outline" disabled={searching || query.trim().length < 2}><SearchIcon aria-hidden="true" />{t(searching ? "skills.hub.searching" : "skills.hub.search")}</Button>
        </form>
        {hits !== null && (hits.length === 0 ? <p className="text-sm text-muted-foreground">{t("skills.hub.none")}</p> : (
          <ul className="grid gap-2">
            {hits.map((hit) => {
              const have = skills.some((skill) => skill.name === hit.name);
              return (
                <li key={hit.id} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2">
                  <div className="grid min-w-0 flex-1 gap-0.5">
                    <span className="text-sm font-medium">{hit.name}</span>
                    <span className="truncate text-xs text-muted-foreground">{hit.source} · {t("skills.hub.installs", { count: hit.installs })}</span>
                  </div>
                  <Button type="button" size="sm" variant="outline" disabled={downloading !== null} onClick={() => void installFromSkillHub(hit)}>
                    <DownloadIcon aria-hidden="true" />{t(downloading === hit.id ? "skills.hub.installing" : have ? "skills.hub.reinstall" : "skills.hub.install")}
                  </Button>
                </li>
              );
            })}
          </ul>
        ))}
        <p className="text-xs leading-snug text-muted-foreground">{t("skills.hub.notes")}</p>
      </SettingsSection>}
      <OrgExtensionsSection kind="skills" />
      <SettingsSection title={t("skills.paste.title")} description={t("skills.paste.description")}>
        <Textarea rows={6} value={pasted} placeholder={t("skills.paste.placeholder")} className="font-mono text-xs" onChange={(event) => setPasted(event.target.value)} />
        <div className="flex justify-end">
          <Button type="button" disabled={!pasted.trim()} onClick={() => void installSkillFromText(pasted).then((ok) => { if (ok) setPasted(""); })}>{t("skills.paste.install")}</Button>
        </div>
      </SettingsSection>
    </div>
  );
}
