import { MAX_BYTES, parseRecording, verifySearchDigests } from './recording';
self.onmessage = async (event: MessageEvent<{ text: string; token: number }>) => {
  try {
    if (new TextEncoder().encode(event.data.text).byteLength > MAX_BYTES) throw new Error('Recording exceeds the 64 MiB viewer limit');
    const recording = parseRecording(JSON.parse(event.data.text));
    await verifySearchDigests(recording);
    self.postMessage({ token: event.data.token, recording });
  } catch (error) { self.postMessage({ token: event.data.token, error: error instanceof Error ? error.message : 'Invalid recording' }); }
};
