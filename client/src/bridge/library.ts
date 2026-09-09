import type { LibraryMenuItems } from '@/types/LibraryMenuItems';

import { invoke } from './runtime';

export const loadLibraryMenuItems = async (): Promise<LibraryMenuItems> => {
  return await invoke<LibraryMenuItems>('load_library_menu_items');
};
