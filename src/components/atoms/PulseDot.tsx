/** O ponto que pulsa enquanto um pedido está em andamento. */
export function PulseDot() {
  return <span className="size-[7px] flex-none self-center rounded-full bg-primary animate-pending-pulse motion-reduce:animate-none" />;
}
