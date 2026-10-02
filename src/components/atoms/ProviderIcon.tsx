import type { SVGProps } from "react";
import type { Provider } from "@/modules/auth";
import { BitbucketIcon, GithubIcon, GitlabIcon } from "./icons";

const ICONS = { github: GithubIcon, gitlab: GitlabIcon, bitbucket: BitbucketIcon };

/** O nome de cada provedor é marca: não se traduz. */
export const PROVIDER_NAMES: Record<Provider, string> = { github: "GitHub", gitlab: "GitLab", bitbucket: "Bitbucket" };

/** A marca de cada provedor de login, onde quer que ele apareça. */
export function ProviderIcon({ provider, ...props }: { provider: Provider } & SVGProps<SVGSVGElement>) {
  const Icon = ICONS[provider];
  return <Icon {...props} />;
}
