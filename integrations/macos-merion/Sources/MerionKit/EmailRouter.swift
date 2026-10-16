import Foundation

public struct RouteSuggestion: Equatable, Sendable {
    public let useCase: String
    public let score: Int
    public let reason: String
}

/// Picks which email use cases fit a message, from words in it. Deterministic and explainable: it names the words it matched.
/// Only use cases the host actually lists are offered; the general mailbox triage is the fallback.
public enum EmailRouter {
    struct Rule { let id: String; let words: [String]; let label: String }
    static let rules: [Rule] = [
        Rule(id: "receivables-ageing", words: ["invoice", "overdue", "unpaid", "amount due", "past due", "outstanding", "payment received", "due date", "inv-", "reminder"], label: "invoices and payments"),
        Rule(id: "travel-itinerary", words: ["flight", "booking reference", "boarding", "departs", "arrives", "itinerary", "check-in", "gate", "confirmation number", "hotel", "pnr"], label: "a trip"),
        Rule(id: "subscription-finder", words: ["renews", "renewal", "subscription", "free trial", "will be charged", "billing", "membership", "your plan", "auto-renew", "trial ends"], label: "renewals and charges"),
        Rule(id: "reimbursement-claims", words: ["reimburse", "reimbursement", "expense claim", "claim", "receipt attached", "cab", "per diem"], label: "expense claims"),
    ]
    public static let fallback = "mailbox-triage"

    public static func route(_ text: String, available: Set<String>) -> [RouteSuggestion] {
        let lower = text.lowercased()
        var out: [RouteSuggestion] = []
        for rule in rules where available.contains(rule.id) {
            let hits = rule.words.filter { lower.contains($0) }
            if hits.count >= 2 { out.append(RouteSuggestion(useCase: rule.id, score: hits.count, reason: "mentions \(hits.prefix(3).joined(separator: ", ")) (\(rule.label))")) }
        }
        out.sort { $0.score == $1.score ? $0.useCase < $1.useCase : $0.score > $1.score }
        if available.contains(fallback) { out.append(RouteSuggestion(useCase: fallback, score: 0, reason: "a general summary: senders, subjects, replies, money, meetings")) }
        return out
    }
}
