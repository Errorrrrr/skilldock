/// <reference types="vite/client" />

declare module 'markdown-it' {
  export default class MarkdownIt {
    constructor(options?: Record<string, unknown>)
    render(source: string): string
  }
}

interface Window {
  __TAURI_INTERNALS__?: unknown
}
