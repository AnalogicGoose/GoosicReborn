#if !os(macOS) || GOOSIC_PREVIEW_NO_WEBKIT
import Foundation

@MainActor
final class PersonalCatalogHost {
    func bind(profileIdentifier: UUID?) {}

    func loadRadio(
        seedVideoID: String, continuation: String? = nil,
        completion: @escaping (Result<GoosicCatalogPage, Error>) -> Void
    ) {
        completion(.failure(PersonalCatalogUnavailable()))
    }

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
        shape: CatalogPageShape = .auto,
        completion: @escaping (Result<GoosicCatalogPage, Error>) -> Void
    ) {
        completion(.failure(PersonalCatalogUnavailable()))
    }

    /// A stub reports the limitation rather than succeeding quietly. Reporting a change as
    /// applied when nothing was sent would leave the screen showing a library the account does
    /// not have.
    func mutate(
        _ mutation: PersonalMutation,
        completion: @escaping (Result<PersonalMutationResult, Error>) -> Void
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
