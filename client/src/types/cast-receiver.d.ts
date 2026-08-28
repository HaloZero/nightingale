// Minimal ambient types for the Cast Application Framework (CAF) receiver
// SDK, loaded via a <script> tag in receiver.html (not an npm package) --
// covers only the APIs client/src/pages/receiver actually calls.
// https://developers.google.com/cast/docs/web_receiver/core_features

declare namespace cast.framework {
  interface CustomMessageEvent<T = unknown> {
    senderId: string;
    data: T;
  }

  interface CastReceiverOptions {
    disableIdleTimeout?: boolean;
  }

  class CastReceiverContext {
    static getInstance(): CastReceiverContext;
    addCustomMessageListener<T = unknown>(
      namespace: string,
      listener: (event: CustomMessageEvent<T>) => void,
    ): void;
    start(options?: CastReceiverOptions): void;
  }
}

interface Window {
  cast?: { framework?: typeof cast.framework };
}
