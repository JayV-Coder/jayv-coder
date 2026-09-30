import type { Aspect } from "@/modules/core";
import { LampIcon } from "@/components/atoms";

/** Uma casa do placar da portaria: lâmpada acesa na cor, número e nome. */
export function TallyItem({ aspect, count, label }: { aspect: Aspect; count: number; label: string }) {
  return (
    <div data-aspect={aspect} className="grid grid-cols-[auto_auto] items-center gap-x-2.5 border-s border-rail-2 px-[15px] py-0.5 xl:px-6">
      <LampIcon className="lamp row-span-2 size-[15px] text-[#3d454e]" data-aspect={aspect} />
      <b className="font-gate-mono text-[22px] leading-[1.05] font-semibold tracking-[-0.02em] text-[#eef1f4] tabular-nums xl:text-[30px]">{count}</b>
      <span className="text-[13px] text-dim">{label}</span>
    </div>
  );
}
