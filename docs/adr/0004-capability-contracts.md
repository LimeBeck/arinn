# ADR-0004: Capability contracts and no plugin-supplied UI

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0020](0020-dbus-contract-baseline.md) (identity and routing), [ADR-0025](0025-downloads-mvp.md) (sixth capability: DownloadProvider)
- **Specification:** [§9.1, §9.2, §20](../specifications/arinn-specification.md)

## Context

New integrations must be added by installing a provider, not by adding product-name conditions to the core or QML. One TV UI must keep navigation and visual consistency.

## Decision

Plugin API v1 defines five capabilities: ApplicationProvider, BrowserProvider, MediaProvider, LibraryProvider and ActionProvider. A plugin may implement several capabilities; the registry and routing work on capabilities only.

| Capability | Conceptual operations |
| --- | --- |
| ApplicationProvider | listApplications, getApplication, launch, activate, terminate, listRunning |
| BrowserProvider | getWindows, getTabs, open, activateTab, closeTab, createTab, getActiveTab |
| MediaProvider | listSessions, getSession, play, pause, playPause, seek, next, previous, stop |
| LibraryProvider | listCollections, listItems, getItem, launch |
| ActionProvider | generic actions with a machine-readable id, title, icon and enabled flag |

Application and browser IDs are opaque: the core never parses them. For example, `application_id = "steam"` has meaning only inside its provider.

Plugins supply data, metadata, actions and capabilities. They never supply QML, HTML or any other loadable UI. Arinn draws all UI.

If the existing capabilities suffice, a new integration changes neither `arinnd` nor `arinn-shell`. If they do not, the Plugin API changes through a separate review, and the change must apply to more than one product.

## Consequences

Integrations are independent of presentation. Extensions needing new semantic operations go through API review; plugin UI cannot be used to bypass that.

## Alternatives considered

Product-specific branches and types such as SteamLibrary or JellyfinLibrary contradict the specification. ActionProvider is for generic actions and does not allow arbitrary code injection.

## Verification

AC-11, AC-12; R-01, R-04, R-09, R-10. Installing a provider with existing capabilities needs no shell or core change. The operation lists above are conceptual, not frozen IPC signatures.
