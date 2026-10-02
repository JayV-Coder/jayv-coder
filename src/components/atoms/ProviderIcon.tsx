import type { SVGProps } from "react";
import { PROVIDER_NAMES, type Provider } from "@/modules/auth";
import { BitbucketIcon, GithubIcon, GitlabIcon } from "./icons";

const ICONS = { github: GithubIcon, gitlab: GitlabIcon, bitbucket: BitbucketIcon };

export { PROVIDER_NAMES };

/** A marca de cada provedor de login, onde quer que ele apareça. */
export function ProviderIcon({ provider, ...props }: { provider: Provider } & SVGProps<SVGSVGElement>) {
  const Icon = ICONS[provider];
  return <Icon {...props} />;
}
