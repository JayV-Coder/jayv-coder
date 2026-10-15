// Os plugins do Tauri que a tela usa, sem efeito: o estúdio só desenha.
export async function openUrl() {}
export async function openPath() {}
export async function open() { return null; }
export async function save() { return null; }
export async function message() {}
export async function ask() { return false; }
export async function confirm() { return false; }
export async function relaunch() {}
export async function exit() {}
export async function check() { return null; }
export async function getCurrent() { return null; }
export async function onOpenUrl() { return () => undefined; }
export type Update = never;
