namespace VoiceInput.Protocol;

public enum CapsuleState
{
    Sleeping,
    Idle,
    Recording,
    Transcribing,
    RetryLocal,
    Ready,
    Error,
}
