/// <reference types="vite/client" />
declare module "*?script" { const path: string; export default path; }
declare module "*?worker&url" { const url: string; export default url; }
declare module "*.css";
