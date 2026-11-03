import AVFoundation
import Foundation
import Speech

/// Listens through the microphone and transcribes speech in the chosen language, on the device when the language allows it.
@MainActor
final class VoiceListener: ObservableObject {
    @Published var transcript = ""
    @Published var listening = false
    @Published var level: Float = 0
    @Published var problem: String?

    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var silence: Task<Void, Never>?
    var onFinished: ((String) -> Void)?

    static func locales() -> [Locale] {
        SFSpeechRecognizer.supportedLocales().sorted { (Locale.current.localizedString(forIdentifier: $0.identifier) ?? $0.identifier) < (Locale.current.localizedString(forIdentifier: $1.identifier) ?? $1.identifier) }
    }

    /// The supported speech locale closest to `preferred` (same language and region, else same language, else US English),
    /// so the picker always has a matching entry.
    static func bestLocaleID(_ preferred: String) -> String {
        let all = SFSpeechRecognizer.supportedLocales()
        let want = Locale(identifier: preferred)
        if let m = all.first(where: { $0.identifier == preferred || $0.identifier(.bcp47) == want.identifier(.bcp47) }) { return m.identifier }
        if let m = all.first(where: { $0.language.languageCode == want.language.languageCode && $0.region == want.region }) { return m.identifier }
        if let m = all.first(where: { $0.language.languageCode == want.language.languageCode }) { return m.identifier }
        return all.first(where: { $0.identifier.hasPrefix("en_US") || $0.identifier.hasPrefix("en-US") })?.identifier ?? all.first?.identifier ?? "en_US"
    }

    func permissionProblem() async -> String? {
        let speech: SFSpeechRecognizerAuthorizationStatus = await withCheckedContinuation { c in SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) } }
        guard speech == .authorized else { return "Speech Recognition is not allowed. Turn it on for Solvor in System Settings, Privacy & Security, Speech Recognition." }
        let mic = await AVCaptureDevice.requestAccess(for: .audio)
        guard mic else { return "Microphone access is not allowed. Turn it on for Solvor in System Settings, Privacy & Security, Microphone." }
        return nil
    }

    func start(locale: Locale) async {
        problem = nil; transcript = ""
        if let p = await permissionProblem() { problem = p; return }
        guard let recognizer = SFSpeechRecognizer(locale: locale), recognizer.isAvailable else { problem = "Speech recognition is not available for that language right now."; return }
        let req = SFSpeechAudioBufferRecognitionRequest()
        req.shouldReportPartialResults = true
        if recognizer.supportsOnDeviceRecognition { req.requiresOnDeviceRecognition = true }
        request = req
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        input.removeTap(onBus: 0)
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, _ in
            req.append(buffer)
            let n = Int(buffer.frameLength)
            guard n > 0, let ch = buffer.floatChannelData?[0] else { return }
            var sum: Float = 0; for i in 0..<n { sum += ch[i] * ch[i] }
            let rms = (sum / Float(n)).squareRoot()
            Task { @MainActor in self?.level = min(1, rms * 12) }
        }
        do { engine.prepare(); try engine.start() } catch { problem = "The microphone could not be started: \(error.localizedDescription)"; return }
        listening = true
        task = recognizer.recognitionTask(with: req) { [weak self] result, error in
            Task { @MainActor in
                guard let self else { return }
                if let result {
                    self.transcript = result.bestTranscription.formattedString
                    self.armSilenceTimer()
                    if result.isFinal { self.finish() }
                }
                if error != nil, self.listening { self.finish() }
            }
        }
    }

    /// Stop when nothing new has been heard for two seconds.
    private func armSilenceTimer() {
        silence?.cancel()
        silence = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 2_000_000_000)
            if !Task.isCancelled { await MainActor.run { self?.finish() } }
        }
    }

    func finish() {
        guard listening else { return }
        listening = false; silence?.cancel()
        engine.stop(); engine.inputNode.removeTap(onBus: 0)
        request?.endAudio(); task?.cancel(); request = nil; task = nil; level = 0
        let heard = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        if !heard.isEmpty { onFinished?(heard) }
    }
}
