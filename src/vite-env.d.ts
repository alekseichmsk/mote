/// <reference types="vite/client" />

declare module "*.riv?url" {
  const src: string;
  export default src;
}

declare module "*.riv" {
  const src: string;
  export default src;
}
