import { LogOutIcon } from "lucide-react";
import type { Expertise } from "@/modules/core";
import { signOut, type Profile } from "@/modules/auth";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { UserAvatar } from "@/components/atoms";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";

/** O nome que se mostra: o do GitHub ou, sem ele, o começo do e-mail. */
export function displayName(profile: Profile | null, email: string | null) {
  return profile?.name ?? email?.split("@")[0] ?? "";
}

/** Quem está conectado: foto, nome, e-mail, como entrou e desde quando. */
export function ProfileCard({ email, profile, expertise }: { email: string | null; profile: Profile | null; expertise: Expertise | null }) {
  const t = useT();
  const locale = useLocale();
  const name = displayName(profile, email);
  const date = (iso: string | null | undefined) => (iso ? new Intl.DateTimeFormat(locale, { dateStyle: "long" }).format(new Date(iso)) : null);
  const since = date(profile?.createdAt);
  const last = date(profile?.lastSignInAt);
  const provider = profile?.provider === "github" ? "GitHub" : t("profile.provider.email");

  return (
    <Card className="relative mb-5 gap-0 overflow-hidden p-0">
      <div aria-hidden="true" className="h-20 bg-[radial-gradient(120%_140%_at_0%_0%,#2b5a33_0%,#142118_45%,transparent_80%)]" />
      <div className="flex flex-wrap items-end gap-5 px-7 pb-6">
        <UserAvatar name={name} src={profile?.avatarUrl} className="-mt-10 size-[84px] border-4 border-card text-[32px]" />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-[25px] font-bold tracking-[-0.02em]">{name}</h2>
          {email && <p className="truncate text-sm text-muted-foreground">{email}</p>}
          <div className="mt-2.5 flex flex-wrap items-center gap-2">
            <Badge variant="outline">{t("profile.provider", { provider })}</Badge>
            {expertise && <Badge variant="outline" className="border-[#4e6353] text-[#a4f4a9]">{t(`expertise.${expertise}` as Key)}</Badge>}
          </div>
        </div>
        <Button variant="outline" onClick={() => void signOut()} className="hover:border-destructive/60 hover:text-destructive">
          <LogOutIcon />
          {t("auth.signOut")}
        </Button>
      </div>
      {(since || last) && (
        <dl className="grid gap-x-8 gap-y-1 border-t border-border/60 px-7 py-3.5 text-xs sm:grid-cols-2">
          {since && <div className="flex gap-2"><dt className="text-muted-foreground">{t("profile.memberSince")}</dt><dd>{since}</dd></div>}
          {last && <div className="flex gap-2"><dt className="text-muted-foreground">{t("profile.lastSignIn")}</dt><dd>{last}</dd></div>}
        </dl>
      )}
    </Card>
  );
}
