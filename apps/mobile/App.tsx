import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { Button, StyleSheet, Text, View } from 'react-native';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { cosyncNative } from './src/native/cosync';

export default function App() {
  const [permission, requestPermission] = useCameraPermissions();
  const [scanned, setScanned] = useState(false);
  const [status, setStatus] = useState('Scan a Cosync pairing QR code.');

  useEffect(() => {
    if (!scanned) return;
    let mounted = true;
    cosyncNative.isConnected().then((connected) => {
      if (mounted && connected) setStatus('Connected to desktop.');
    });
    const subscription = cosyncNative.onConnectionState((connected) => {
      if (connected) setStatus('Connected to desktop.');
      else setStatus('Disconnected from desktop.');
    });
    return () => { mounted = false; subscription.remove(); };
  }, [scanned]);

  if (!permission) return <View style={styles.center}><Text>Requesting camera permission…</Text></View>;
  if (!permission.granted) return <View style={styles.center}>
    <Text style={styles.title}>Cosync</Text>
    <Text style={styles.body}>Camera access is required to scan the desktop pairing code.</Text>
    <Button title="Allow camera" onPress={requestPermission} />
    <StatusBar style="light" />
  </View>;

  return <View style={styles.container}>
    <CameraView style={StyleSheet.absoluteFill} facing="back"
      barcodeScannerSettings={{ barcodeTypes: ['qr'] }}
      onBarcodeScanned={scanned ? undefined : async ({ data }) => {
        setScanned(true);
        const result = await cosyncNative.pair(data, 'Android device');
        setStatus(result === 'native bridge unavailable' ? 'Native Cosync bridge is unavailable in this build.' : result);
      }} />
    <View style={styles.overlay}>
      <Text style={styles.title}>Pair with desktop</Text>
      <Text style={styles.body}>{status}</Text>
      {scanned && <Button title="Scan another code" onPress={() => { setScanned(false); setStatus('Scan a Cosync pairing QR code.'); }} />}
    </View>
    <StatusBar style="light" />
  </View>;
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#101522' },
  center: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 16, padding: 24, backgroundColor: '#101522' },
  overlay: { marginTop: 72, marginHorizontal: 24, padding: 20, borderRadius: 16, backgroundColor: 'rgba(16,21,34,0.86)' },
  title: { color: '#fff', fontSize: 24, fontWeight: '700', marginBottom: 8 },
  body: { color: '#d8deec', fontSize: 16, lineHeight: 23 },
});
