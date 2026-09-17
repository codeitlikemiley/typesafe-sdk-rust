#![allow(dead_code)]

use serde_json::{Value, json};

pub fn fan_out_triage() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 400, "output_tokens": 80},
        "answers": {
            "category": {
                "type": "choice",
                "choice": "bug_report",
                "confidence": 0.88,
                "probabilities": {
                    "bug_report": 0.72,
                    "billing": 0.15,
                    "feature_request": 0.13
                }
            },
            "bug_severity": {
                "type": "score",
                "score": 2.1,
                "confidence": 0.75,
                "legend": {"0": "cosmetic", "1": "annoying", "2": "blocking"},
                "probabilities": {"0": 0.05, "1": 0.2, "2": 0.75}
            },
            "has_reproducible_steps": {"type": "noul", "noul": 0.82},
            "refund_requested": {"type": "noul", "noul": 0.12},
            "frustration": {
                "type": "score",
                "score": 1.6,
                "confidence": 0.7,
                "legend": {"0": "calm", "1": "frustrated", "2": "angry"},
                "probabilities": {"0": 0.1, "1": 0.55, "2": 0.35}
            }
        }
    })
}

pub fn confidence_routing() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 200, "output_tokens": 40},
        "answers": {
            "action": {
                "type": "choice",
                "choice": "approve_transfer",
                "confidence": 0.62,
                "probabilities": {
                    "check_balance": 0.05,
                    "approve_transfer": 0.62,
                    "support": 0.33
                }
            }
        }
    })
}

pub fn guardrails() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 180, "output_tokens": 35},
        "answers": {
            "jailbreak": {"type": "noul", "noul": 0.91},
            "harm_severity": {
                "type": "score",
                "score": 2.4,
                "confidence": 0.85,
                "legend": {"0": "none", "1": "moderate", "2": "severe"},
                "probabilities": {"0": 0.02, "1": 0.18, "2": 0.8}
            }
        }
    })
}

pub fn citation_check() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 350, "output_tokens": 30},
        "answers": {
            "supports_claim": {
                "type": "choice",
                "choice": "partially",
                "confidence": 0.45,
                "probabilities": {
                    "yes": 0.2,
                    "partially": 0.45,
                    "no": 0.35
                }
            }
        }
    })
}

pub fn structured_ticket() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 500, "output_tokens": 50},
        "answers": {
            "duplicate_charge": {"type": "noul", "noul": 0.97},
            "refund_eligible": {"type": "noul", "noul": 0.88}
        }
    })
}

pub fn composite_scores() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 220, "output_tokens": 45},
        "answers": {
            "clarity": {
                "type": "score",
                "score": 2.0,
                "confidence": 0.8,
                "legend": {"0": "poor", "1": "ok", "2": "excellent"},
                "probabilities": {"0": 0.05, "1": 0.15, "2": 0.8}
            },
            "completeness": {
                "type": "score",
                "score": 1.2,
                "confidence": 0.72,
                "legend": {"0": "missing", "1": "partial", "2": "complete"},
                "probabilities": {"0": 0.1, "1": 0.6, "2": 0.3}
            },
            "tone": {
                "type": "score",
                "score": 1.8,
                "confidence": 0.77,
                "legend": {"0": "harsh", "1": "neutral", "2": "warm"},
                "probabilities": {"0": 0.08, "1": 0.22, "2": 0.7}
            }
        }
    })
}

pub fn intent_routing() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 150, "output_tokens": 25},
        "answers": {
            "intent": {
                "type": "choice",
                "choice": "specialist_llm",
                "confidence": 0.79,
                "probabilities": {
                    "deterministic": 0.08,
                    "specialist_llm": 0.79,
                    "human": 0.13
                }
            }
        }
    })
}

pub fn noul_uncertainty_band() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 120, "output_tokens": 20},
        "answers": {
            "fraud_likely": {"type": "noul", "noul": 0.48}
        }
    })
}

pub fn mixed_primitives() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 12, "output_tokens": 3},
        "answers": {
            "billing": {"type": "noul", "noul": 0.98},
            "tone": {
                "type": "choice",
                "choice": "frustrated",
                "confidence": 0.9,
                "probabilities": {"calm": 0.05, "frustrated": 0.9, "angry": 0.05}
            },
            "urgency": {
                "type": "score",
                "score": 2.2,
                "confidence": 0.85,
                "legend": {"0": "can wait", "1": "this week", "2": "today"},
                "probabilities": {"0": 0.05, "1": 0.15, "2": 0.8}
            }
        }
    })
}
