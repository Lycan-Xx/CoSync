import { DeviceEventEmitter, NativeModules } from 'react-native';

type CosyncNativeModule = {
  pair(payload: string, deviceName: string): Promise<string>;
  isConnected(): Promise<boolean>;
  disconnect(): Promise<void>;
};

const nativeModule = NativeModules.Cosync as CosyncNativeModule | undefined;

export const cosyncNative = {
  available: nativeModule !== undefined,
  pair: (payload: string, deviceName: string) => {
    if (!nativeModule) return Promise.resolve('native bridge unavailable');
    return nativeModule.pair(payload, deviceName);
  },
  isConnected: () => nativeModule?.isConnected() ?? Promise.resolve(false),
  disconnect: () => nativeModule?.disconnect() ?? Promise.resolve(),
  onConnectionState: (listener: (connected: boolean) => void) =>
    DeviceEventEmitter.addListener('cosyncConnectionState', listener),
};
