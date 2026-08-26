export {};

interface MyUrlTurnstile {
  render: (
    element: HTMLElement,
    options: {
      sitekey: string;
      action: string;
      callback: (token: string) => void;
      'error-callback': () => void;
      'expired-callback': () => void;
    },
  ) => string;
  reset?: (widgetId?: string) => void;
}

declare global {
  interface Window {
    turnstile?: MyUrlTurnstile;
  }
}
