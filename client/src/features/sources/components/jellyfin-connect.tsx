import { RemoteSourceConnectDialog } from '@/features/sources/components/connect';
import {
  useConnectJellyfin,
  useJellyfinLogin,
} from '@/features/sources/mutations/use-source-mutations';

export const JellyfinConnectDialog = () => {
  const loginMutation = useJellyfinLogin();
  const connectMutation = useConnectJellyfin();

  return (
    <RemoteSourceConnectDialog
      mode="jellyfin-connect"
      title="Connect to Jellyfin"
      description={
        <>
          Point Nightingale at a Jellyfin server. Audio is downloaded to your local cache on first
          analysis so the rest of the karaoke pipeline keeps working exactly like a folder library.
        </>
      }
      urlInputId="jelly-url"
      urlPlaceholder="https://jellyfin.example.com"
      usernameInputId="jelly-user"
      passwordInputId="jelly-pass"
      loginMutation={loginMutation}
      connectMutation={connectMutation}
      selection={{
        label: 'Libraries to import',
        emptyMessage:
          "This server didn't report any libraries, so everything the account can see will be imported.",
        getItems: (login) =>
          login.libraries.map((library) => ({ id: library.id, label: library.name })),
      }}
    />
  );
};
