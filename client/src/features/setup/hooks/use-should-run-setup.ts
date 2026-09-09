import { atom, useAtom } from 'jotai';
import { useEffect } from 'react';

import { isAppReady } from '@/bridge/setup';

const shouldRunSetupAtom = atom(false);

export const useShouldRunSetup = () => {
  const [shouldRunSetup, setShouldRunSetup] = useAtom(shouldRunSetupAtom);

  useEffect(() => {
    // Check, if setup is required
    const checkIsAppReady = async () => {
      return await isAppReady();
    };

    void checkIsAppReady().then((ready) => setShouldRunSetup(!ready));
  }, [setShouldRunSetup]);

  return { shouldRunSetup, setShouldRunSetup };
};
