import { LogOutIcon } from "lucide-react";
import type { Expertise } from "@/modules/core";
import { PROVIDERS, signOut, useAuth, type Profile, type Provider } from "@/modules/auth";
import type { AccountProfile } from "@/modules/profile";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { PROVIDER_NAMES, ProviderIcon, UserAvatar } from "@/components/atoms";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";

/** O nome que se mostra: o do perfil, o do provedor ou o começo do e-mail. */
export function displayName(account: AccountProfile | null, profile: Profile | null, email: string | null) {
  return account?.displayName || profile?.name || email?.split("@")[0] || "";
}

/** Quem está conectado: foto, nome, e-mail, como entrou e desde quando. */
export function ProfileCard({ email, account, profile, expertise }: { email: string | null; account: AccountProfile | null; profile: Profile | null; expertise: Expertise | null }) {
  const t = useT();
  const locale = useLocale();
  const providers = useAuth((state) => state.providers);
  const hasPassword = useAuth((state) => state.hasPassword);
  const linked = PROVIDERS.filter((provider: Provider) => providers.includes(provider));
  const name = displayName(account, profile, email);
  const date = (iso: string | null | undefined) => (iso ? new Intl.DateTimeFormat(locale, { dateStyle: "long" }).format(new Date(iso)) : null);
  const since = date(profile?.createdAt);
  const last = date(profile?.lastSignInAt);
  const provider = (PROVIDERS as string[]).includes(profile?.provider ?? "") ? PROVIDER_NAMES[profile!.provider as Provider] : t("profile.provider.email");

  return (
    <Card className="relative mb-5 gap-0 overflow-hidden p-0">
      <div aria-hidden="true" className="h-20 border-b border-border bg-secondary" />
      <div className="flex flex-wrap items-end gap-5 px-7 pb-6">
        <UserAvatar name={name} src={profile?.avatarUrl} className="-mt-10 size-[84px] border-4 border-card text-h1" />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-h2 font-semibold">{name}</h2>
          {(account?.username || email) && (
            <p className="truncate text-sm text-muted-foreground">
              {account?.username && <span className="font-mono text-foreground/80">@{account.username}</span>}
              {account?.username && email && <span aria-hidden="true"> · </span>}
              {email}
            </p>
          )}
          <div className="mt-2.5 flex flex-wrap items-center gap-2">
            <Badge variant="outline">{t("profile.provider", { provider })}</Badge>
            {hasPassword && <Badge variant="outline">{t("profile.password")}</Badge>}
            {linked.map((linkedProvider) => (
              <Badge key={linkedProvider} variant="outline"><ProviderIcon provider={linkedProvider} />{PROVIDER_NAMES[linkedProvider]}</Badge>
            ))}
            {expertise && <Badge variant="outline" className="border-muted-foreground/50 text-success">{t(`expertise.${expertise}` as Key)}</Badge>}
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
