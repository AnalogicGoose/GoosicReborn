#if !os(macOS)
import Foundation

@MainActor
final class PersonalCatalogHost {
    func bind(profileIdentifier: UUID?) {}

    func load(
        section: PersonalLibrarySection,
        continuation: String? = nil,
        completion: @escaping (Result<GoosicCatalogPage, Error>) -> Void
    ) {
        completion(.failure(PersonalCatalogUnavailable()))
    }

    func loadBrowse(
        browseID: String,
        title: String,
        continuation: String? = nil,
        completion: @escaping (Result<GoosicCatalogPage, Error>) -> Void
    ) {
        completion(.failure(PersonalCatalogUnavailable()))
    }
}

private struct PersonalCatalogUnavailable: LocalizedError {
    var errorDescription: String? {
        "Personal library browsing is not available on this platform yet."
    }
}
#endif
