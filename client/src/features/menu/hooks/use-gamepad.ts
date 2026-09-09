import { useCallback, useEffect, useRef } from 'react';

import { useLatestRef } from '@/shared/hooks/use-latest-ref';

const STICK_DEADZONE = 0.5;
const STICK_INITIAL_DELAY = 0.4;
const STICK_REPEAT_RATE = 0.08;

type StickAxis = 'up' | 'down' | 'left' | 'right';

type RepeatState = {
  axis: StickAxis | null;
  timer: number;
  started: boolean;
};

type GamepadInput = {
  confirm: boolean;
  back: boolean;
  upHeld: boolean;
  downHeld: boolean;
  leftHeld: boolean;
  rightHeld: boolean;
  stickDir: StickAxis | null;
};

export type GamepadSnapshot = {
  up: boolean;
  down: boolean;
  left: boolean;
  right: boolean;
  confirm: boolean;
  back: boolean;
  upHeld: boolean;
  downHeld: boolean;
};

const INITIAL_REPEAT: RepeatState = { axis: null, timer: 0, started: false };

function tickRepeat(
  rep: RepeatState,
  dir: StickAxis | null,
  dt: number,
): { up: boolean; down: boolean; left: boolean; right: boolean } {
  const result = { up: false, down: false, left: false, right: false };

  if (!dir) {
    rep.axis = null;
    rep.started = false;
    return result;
  }

  if (rep.axis !== dir) {
    rep.axis = dir;
    rep.timer = STICK_INITIAL_DELAY;
    rep.started = true;
    result[dir] = true;
  } else if (rep.started) {
    rep.timer -= dt;
    if (rep.timer <= 0) {
      rep.timer = STICK_REPEAT_RATE;
      result[dir] = true;
    }
  }

  return result;
}

function pressedButtons(gamepad: Gamepad): Set<number> {
  const pressed = new Set<number>();

  for (let index = 0; index < gamepad.buttons.length; index++) {
    if (gamepad.buttons[index].pressed) {
      pressed.add(index);
    }
  }

  return pressed;
}

function stickDirection(gamepad: Gamepad): StickAxis | null {
  if (gamepad.axes.length < 2) {
    return null;
  }

  const x = gamepad.axes[0];
  const y = gamepad.axes[1];
  if (Math.abs(y) <= STICK_DEADZONE && Math.abs(x) <= STICK_DEADZONE) {
    return null;
  }

  if (Math.abs(y) >= Math.abs(x)) {
    return y < 0 ? 'up' : 'down';
  }

  return x < 0 ? 'left' : 'right';
}

function mergeGamepadInput(
  input: GamepadInput,
  gamepad: Gamepad,
  current: ReadonlySet<number>,
  previous: ReadonlySet<number>,
): void {
  const justPressed = (index: number) => current.has(index) && !previous.has(index);
  input.confirm ||= justPressed(0);
  input.back ||= justPressed(1) || justPressed(9);
  input.upHeld ||= current.has(12);
  input.downHeld ||= current.has(13);
  input.leftHeld ||= current.has(14);
  input.rightHeld ||= current.has(15);
  input.stickDir = stickDirection(gamepad) ?? input.stickDir;
}

function readGamepads(previousButtons: Map<number, Set<number>>): GamepadInput {
  const input: GamepadInput = {
    confirm: false,
    back: false,
    upHeld: false,
    downHeld: false,
    leftHeld: false,
    rightHeld: false,
    stickDir: null,
  };

  for (const gamepad of navigator.getGamepads()) {
    if (!gamepad) {
      continue;
    }

    const previous = previousButtons.get(gamepad.index) ?? new Set<number>();
    const current = pressedButtons(gamepad);
    mergeGamepadInput(input, gamepad, current, previous);
    previousButtons.set(gamepad.index, current);
  }

  return input;
}

function heldDirection(input: GamepadInput): StickAxis | null {
  if (input.upHeld) {
    return 'up';
  }
  if (input.downHeld) {
    return 'down';
  }
  if (input.leftHeld) {
    return 'left';
  }
  if (input.rightHeld) {
    return 'right';
  }

  return null;
}

export function useGamepad(onSnapshot: (snap: GamepadSnapshot) => void) {
  const lastTime = useRef(0);
  const callbackRef = useLatestRef(onSnapshot);
  const prevButtons = useRef<Map<number, Set<number>>>(new Map());
  const stickRepeat = useRef<RepeatState>({ ...INITIAL_REPEAT });
  const dpadRepeat = useRef<RepeatState>({ ...INITIAL_REPEAT });

  const poll = useCallback((time: number): GamepadSnapshot => {
    const dt = lastTime.current === 0 ? 0 : (time - lastTime.current) / 1000;
    lastTime.current = time;

    const input = readGamepads(prevButtons.current);
    const dpad = tickRepeat(dpadRepeat.current, heldDirection(input), dt);
    const stick = tickRepeat(stickRepeat.current, input.stickDir, dt);

    return {
      up: dpad.up || stick.up,
      down: dpad.down || stick.down,
      left: dpad.left || stick.left,
      right: dpad.right || stick.right,
      confirm: input.confirm,
      back: input.back,
      upHeld: input.upHeld || input.stickDir === 'up',
      downHeld: input.downHeld || input.stickDir === 'down',
    };
  }, []);

  useEffect(() => {
    let rafId: number;

    const loop = (time: number) => {
      const snap = poll(time);
      const hasAction = Object.values(snap).some(Boolean);

      if (hasAction) {
        callbackRef.current(snap);
      }

      rafId = requestAnimationFrame(loop);
    };

    rafId = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(rafId);
  }, [callbackRef, poll]);
}
