/// <reference types="vite/client" />

declare const __BUILD_STAMP__: string;
declare const __BUILD_HEAD__: string;
declare const __BUILD_HISTORY__: ReadonlyArray<{
  hash: string;
  date: string;
  subject: string;
}>;
