import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';

import { microphoneAdapter, type MicrophoneAdapter } from '@/bridge/microphone';
import { MIC_DEVICES } from '@/shared/query-keys';
import type { MicrophoneInfo } from '@/types/MicrophoneInfo';

export type MicDevice = {
  deviceId: string;
  label: string;
  name: string;
};

const MIC_DEVICE_CACHE_MS = 60_000;
const EMPTY_MIC_DEVICES: MicDevice[] = [];

const browserMediaDevices = (): MediaDevices | undefined => {
  if (typeof navigator === 'undefined') {
    return undefined;
  }
  return navigator.mediaDevices;
};

async function listMicDevices(adapter: MicrophoneAdapter): Promise<MicDevice[]> {
  const mics = await adapter.listDevices();
  return mics.map(({ id, name, host }: MicrophoneInfo) => ({
    deviceId: id,
    label: host === 'Browser' ? name : `${host}: ${name}`,
    name,
  }));
}

export function useMicDevicesQuery(adapter: MicrophoneAdapter = microphoneAdapter) {
  const queryClient = useQueryClient();

  useEffect(() => {
    const mediaDevices = browserMediaDevices();
    if (!mediaDevices) {
      return undefined;
    }

    const refresh = () => {
      void queryClient.invalidateQueries({ queryKey: MIC_DEVICES });
    };
    mediaDevices.addEventListener('devicechange', refresh);
    return () => {
      mediaDevices.removeEventListener('devicechange', refresh);
    };
  }, [queryClient]);

  const query = useQuery({
    queryKey: MIC_DEVICES,
    queryFn: () => listMicDevices(adapter),
    staleTime: MIC_DEVICE_CACHE_MS,
    cacheTime: MIC_DEVICE_CACHE_MS,
    retry: false,
    // Unlike initialData, placeholderData does not mark an empty list as a
    // successful, fresh response and therefore does not suppress enumeration.
    placeholderData: [],
  });
  return { ...query, data: query.data ?? EMPTY_MIC_DEVICES };
}

export function useMicDevices(adapter: MicrophoneAdapter = microphoneAdapter) {
  return useMicDevicesQuery(adapter).data;
}
