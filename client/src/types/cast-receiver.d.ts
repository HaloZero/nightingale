// Minimal ambient types for the Cast Application Framework (CAF) receiver
// SDK, loaded via a <script> tag in receiver.html (not an npm package) --
// covers only the APIs client/src/pages/receiver actually calls.
// https://developers.google.com/cast/docs/web_receiver/core_features
//
// Deliberately plain interfaces rather than `declare namespace cast.framework`:
// a bare top-level `namespace` declaration also creates a real value on
// `globalThis`, which -- since `window`'s type is `Window & typeof
// globalThis` -- makes TypeScript treat `window.cast` as always-present even
// though `Window.cast` below is declared optional (the script tag may not
// have loaded yet when this runs). Plain interfaces carry no such value,
// so `window.cast?.framework` stays genuinely optional.

interface CastCustomMessageEvent<T = unknown> {
  senderId: string;
  data: T;
}

interface CastReceiverOptions {
  disableIdleTimeout?: boolean;
}

interface CastReceiverContextInstance {
  addCustomMessageListener<T = unknown>(
    namespace: string,
    listener: (event: CastCustomMessageEvent<T>) => void,
  ): void;
  start(options?: CastReceiverOptions): void;
}

interface CastReceiverContextStatic {
  getInstance(): CastReceiverContextInstance;
}

interface Window {
  cast?: {
    framework?: {
      CastReceiverContext: CastReceiverContextStatic;
    };
  };
}
