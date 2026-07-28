import { useCallback, useEffect, useRef } from 'react';

/**
 * A stable predicate answering "is this component still mounted?".
 *
 * Guard state writes that happen after an `await`. A transport call that
 * settles after unmount must not schedule a React update: in the app it is
 * wasted work, and in vitest the environment can already be torn down by the
 * time the rejection lands, so the scheduled update dereferences a deleted
 * `window` and takes down whichever unrelated CI run drew the slow machine
 * (#112).
 *
 * The effect body re-arms the flag so StrictMode's mount → cleanup → mount
 * double-invoke does not leave a live component reading `false`.
 */
export function useAlive(): () => boolean {
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  return useCallback(() => alive.current, []);
}
