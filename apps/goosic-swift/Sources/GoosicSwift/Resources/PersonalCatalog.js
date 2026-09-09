/*
 * PersonalCatalog.js — account-scoped YouTube Music reads, run inside the signed-in
 * WebKit profile by PersonalCatalogHost.
 *
 * Ported from the previous Goosic application, src/lib/innertube/{shared,home,library,
 * library-pagination,playlist,album,mutations,types}.ts, and adapted to run as a page program: the request
 * context and API key come from the page's own `ytcfg`, the session cookie never leaves the
 * document, and the result is projected into the small catalog shape the shell renders.
 *
 * Copyright (C) Oscar Mantilla, George Shyshov, and Goosic contributors.
 *
 * This program is free software: you can redistribute it and/or modify it under the terms
 * of the GNU General Public License as published by the Free Software Foundation, either
 * version 3 of the License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
 * without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 * See the GNU General Public License for more details. A copy is in LICENSE-GPL-3.0 at the
 * repository root.
 *
 * Evaluated as the body of an async function (WKWebView.callAsyncJavaScript), so only
 * `const` bindings are introduced and nothing is attached to `window`. The host appends
 * `return await GoosicPersonalCatalog.browse(browseId, title, continuation);`.
 */
const GoosicPersonalCatalog = (() => {
  const ORIGIN = "https://music.youtube.com";

  // ---------------------------------------------------------------------------------------
  // Request layer. Same-origin POSTs to InnerTube carry the page's cookies automatically;
  // what youtubei additionally requires for an account-scoped answer is the SAPISIDHASH
  // authorization derived from the SAPISID cookie, the session index, and — when the
  // account acts as a brand channel — the delegated page id. All of it is read from the
  // page and used only for this request.
  // ---------------------------------------------------------------------------------------
  const cfg = (key) => {
    try {
      const c = window.ytcfg;
      if (c && typeof c.get === "function") return c.get(key);
      return c?.data_?.[key];
    } catch (_) {
      return undefined;
    }
  };

  const readCookie = (name) => {
    const match = document.cookie.match(new RegExp("(?:^|;\\s*)" + name + "=([^;]+)"));
    return match ? match[1] : null;
  };

  async function sha1Hex(text) {
    const digest = await crypto.subtle.digest("SHA-1", new TextEncoder().encode(text));
    return Array.from(new Uint8Array(digest)).map((b) => b.toString(16).padStart(2, "0")).join("");
  }

  async function authHeaders() {
    const headers = {};
    const sapisid = readCookie("__Secure-3PAPISID") || readCookie("SAPISID");
    if (sapisid) {
      const ts = Math.floor(Date.now() / 1000);
      headers.Authorization = `SAPISIDHASH ${ts}_${await sha1Hex(`${ts} ${sapisid} ${ORIGIN}`)}`;
    }
    const sessionIndex = cfg("SESSION_INDEX");
    headers["X-Goog-AuthUser"] = sessionIndex != null ? String(sessionIndex) : "0";
    const pageId = cfg("DELEGATED_SESSION_ID");
    if (pageId) headers["X-Goog-PageId"] = String(pageId);
    return headers;
  }

  async function innertubePost(endpoint, body) {
    const context = cfg("INNERTUBE_CONTEXT");
    const key = cfg("INNERTUBE_API_KEY");
    if (!context || !key) throw new Error("YouTube Music session context is unavailable");
    const headers = {
      "Content-Type": "application/json",
      "X-Origin": ORIGIN,
      "X-YouTube-Client-Name": String(cfg("INNERTUBE_CONTEXT_CLIENT_NAME") || "67"),
      "X-YouTube-Client-Version": String(context?.client?.clientVersion || ""),
      ...(await authHeaders()),
    };
    const visitor = cfg("VISITOR_DATA") || context?.client?.visitorData;
    if (visitor) headers["X-Goog-Visitor-Id"] = String(visitor);
    const response = await fetch(`${ORIGIN}/youtubei/v1/${endpoint}?prettyPrint=false&key=${encodeURIComponent(key)}`, {
      method: "POST",
      credentials: "include",
      headers,
      body: JSON.stringify({ context, ...body }),
    });
    if (!response.ok) {
      const text = await response.text().catch(() => "");
      throw new Error(`InnerTube ${endpoint} → HTTP ${response.status}: ${text.slice(0, 200)}`);
    }
    return response.json();
  }

  const rawBrowse = (browseId, params) => innertubePost("browse", params ? { browseId, params } : { browseId });
  const rawBrowseContinuation = (token) => innertubePost("browse", { continuation: token });

  // ---------------------------------------------------------------------------------------
  // Response readers (shared.ts).
  // ---------------------------------------------------------------------------------------
  function findContinuationToken(root) {
    const seen = new WeakSet();
    let result;
    const walk = (node) => {
      if (result || !node || typeof node !== "object") return;
      if (seen.has(node)) return;
      seen.add(node);
      if (Array.isArray(node)) {
        for (const child of node) {
          if (result) return;
          walk(child);
        }
        return;
      }
      const t1 = node.nextContinuationData?.continuation;
      if (t1) { result = t1; return; }
      const t2 = node.continuationCommand?.token;
      if (t2) { result = t2; return; }
      for (const key of Object.keys(node)) {
        if (result) return;
        walk(node[key]);
      }
    };
    walk(root);
    return result;
  }

  function readRuns(node) {
    if (!node) return "";
    if (typeof node === "string") return node;
    if (typeof node.simpleText === "string") return node.simpleText;
    if (Array.isArray(node.runs)) return node.runs.map((r) => r?.text ?? "").join("");
    return "";
  }

  function readThumbnails(node) {
    if (!node) return [];
    const arr = node.thumbnails ?? node.thumbnail?.thumbnails ?? [];
    if (!Array.isArray(arr)) return [];
    return arr
      .map((t) => ({ url: t?.url, width: t?.width, height: t?.height }))
      .filter((t) => typeof t.url === "string" && t.url.length > 0);
  }

  function deepFindThumbnails(node) {
    if (!node || typeof node !== "object") return [];
    const seen = new WeakSet();
    let result = [];
    const walk = (n) => {
      if (result.length || !n || typeof n !== "object") return;
      if (seen.has(n)) return;
      seen.add(n);
      if (Array.isArray(n)) { for (const c of n) walk(c); return; }
      if (Array.isArray(n.thumbnails) && n.thumbnails.length) {
        const mapped = n.thumbnails
          .map((t) => ({ url: t?.url, width: t?.width, height: t?.height }))
          .filter((t) => typeof t.url === "string" && t.url.length > 0);
        if (mapped.length) { result = mapped; return; }
      }
      for (const k of Object.keys(n)) walk(n[k]);
    };
    walk(node);
    return result;
  }

  function readExplicit(raw) {
    for (const bucket of [raw.badges ?? [], raw.subtitleBadges ?? []]) {
      for (const b of bucket) {
        const r = b?.musicInlineBadgeRenderer;
        if (!r) continue;
        const iconType = r.icon?.iconType;
        if (typeof iconType === "string" && iconType.includes("EXPLICIT")) return true;
        const label = r.accessibilityData?.accessibilityData?.label ?? r.accessibilityText;
        if (typeof label === "string" && /explicit/i.test(label)) return true;
      }
    }
    return false;
  }

  const isDurationText = (text) => /^\d{1,2}:\d{2}(:\d{2})?$/.test(text);

  function pageTypeToKind(pageType) {
    if (pageType.includes("ARTIST")) return "artist";
    if (pageType.includes("ALBUM")) return "album";
    if (pageType.includes("PLAYLIST") || pageType.includes("PODCAST_SHOW")) return "playlist";
    return null;
  }

  const browseOf = (endpoint) => endpoint?.browseEndpoint;
  const pageTypeOf = (endpoint) =>
    browseOf(endpoint)?.browseEndpointContextSupportedConfigs?.browseEndpointContextMusicConfig?.pageType ?? "";

  function mapTwoRowItem(raw) {
    const endpoint = raw.navigationEndpoint ?? {};
    const browseEndpoint = endpoint.browseEndpoint;
    const watchEndpoint = endpoint.watchEndpoint;
    const title = readRuns(raw.title);
    const subtitle = readRuns(raw.subtitle);

    const artists = [];
    let album, albumId;
    for (const run of raw.subtitle?.runs ?? []) {
      const browseId = browseOf(run.navigationEndpoint)?.browseId;
      const pageType = pageTypeOf(run.navigationEndpoint);
      if (browseId && pageType.includes("ARTIST")) artists.push({ id: browseId, name: run.text ?? "" });
      else if (browseId && pageType.includes("ALBUM")) { album = run.text ?? album; albumId = browseId; }
    }

    let thumbnails = readThumbnails(
      raw.thumbnailRenderer?.musicThumbnailRenderer?.thumbnail ??
        raw.thumbnail?.musicThumbnailRenderer?.thumbnail ??
        raw.thumbnail,
    );
    if (thumbnails.length === 0) thumbnails = deepFindThumbnails(raw.thumbnailRenderer);
    if (thumbnails.length === 0) thumbnails = deepFindThumbnails(raw.thumbnail);

    let kind = "song";
    let id = "";
    if (watchEndpoint?.videoId) {
      id = watchEndpoint.videoId;
      const widest = thumbnails.reduce((m, t) => ((t.width ?? 0) > (m?.width ?? 0) ? t : m), thumbnails[0]);
      const ratio = widest && widest.width && widest.height ? widest.width / widest.height : 1;
      kind = ratio > 1.4 ? "video" : "song";
    } else if (browseEndpoint?.browseId) {
      id = browseEndpoint.browseId;
      kind = pageTypeToKind(pageTypeOf(endpoint)) ?? null;
    } else {
      const playlistId = endpoint.watchPlaylistEndpoint?.playlistId;
      if (playlistId) { id = playlistId; kind = "playlist"; }
    }
    if (!id || !kind) return null;

    let playableVideoId;
    if (kind === "playlist") {
      playableVideoId = raw.overlay?.musicItemThumbnailOverlayRenderer?.content?.musicPlayButtonRenderer
        ?.playNavigationEndpoint?.watchEndpoint?.videoId;
    }
    return { kind, id, title, subtitle, thumbnails, artists, album, albumId, explicit: readExplicit(raw), playableVideoId };
  }

  function mapResponsiveListItem(raw) {
    const flex = raw.flexColumns ?? [];
    const fixed = raw.fixedColumns ?? [];
    const titleText = flex[0]?.musicResponsiveListItemFlexColumnRenderer?.text;
    const titleCol = titleText?.runs?.[0];
    const title = readRuns(titleText);

    const artists = [];
    let album, albumId;
    for (let i = 1; i < flex.length; i++) {
      const colNode = flex[i]?.musicResponsiveListItemFlexColumnRenderer?.text;
      for (const run of colNode?.runs ?? []) {
        const browseId = browseOf(run.navigationEndpoint)?.browseId;
        const pageType = pageTypeOf(run.navigationEndpoint);
        if (browseId && pageType.includes("ARTIST")) artists.push({ id: browseId, name: run.text ?? "" });
        else if (browseId && pageType.includes("ALBUM")) { album = run.text ?? album; albumId = browseId; }
      }
    }

    let duration = readRuns(fixed[0]?.musicResponsiveListItemFixedColumnRenderer?.text);
    if (!isDurationText(duration)) {
      duration = "";
      const subtitleRuns = flex[1]?.musicResponsiveListItemFlexColumnRenderer?.text?.runs ?? [];
      const last = subtitleRuns[subtitleRuns.length - 1];
      if (typeof last?.text === "string" && isDurationText(last.text)) duration = last.text;
    }

    let thumbnails = readThumbnails(raw.thumbnail?.musicThumbnailRenderer?.thumbnail);
    const videoId =
      titleCol?.navigationEndpoint?.watchEndpoint?.videoId ??
      raw.overlay?.musicItemThumbnailOverlayRenderer?.content?.musicPlayButtonRenderer?.playNavigationEndpoint
        ?.watchEndpoint?.videoId ??
      raw.playlistItemData?.videoId ??
      raw.navigationEndpoint?.watchEndpoint?.videoId;
    if (thumbnails.length === 0 && videoId) {
      thumbnails = [
        { url: `https://i.ytimg.com/vi/${videoId}/mqdefault.jpg`, width: 320, height: 180 },
        { url: `https://i.ytimg.com/vi/${videoId}/hqdefault.jpg`, width: 480, height: 360 },
      ];
    }

    const navBrowseId = browseOf(titleCol?.navigationEndpoint)?.browseId ?? browseOf(raw.navigationEndpoint)?.browseId;
    const navPageType = pageTypeOf(titleCol?.navigationEndpoint) || pageTypeOf(raw.navigationEndpoint);
    const subtitle =
      artists.map((a) => a.name).join(", ") || readRuns(flex[1]?.musicResponsiveListItemFlexColumnRenderer?.text);
    const explicit = readExplicit(raw);

    if (videoId) return { kind: "song", id: videoId, title, subtitle, thumbnails, artists, album, albumId, duration, explicit };
    if (navBrowseId) {
      const kind = pageTypeToKind(navPageType);
      if (!kind) return null;
      return { kind, id: navBrowseId, title, subtitle, thumbnails, artists, explicit };
    }
    return null;
  }

  function mapCardShelfFeatured(card) {
    const videoId = card.onTap?.watchEndpoint?.videoId;
    if (!videoId) return null;
    const title = readRuns(card.title);
    if (!title) return null;
    return {
      kind: "song", id: videoId, title, subtitle: readRuns(card.subtitle),
      thumbnails: readThumbnails(card.thumbnail?.musicThumbnailRenderer?.thumbnail), artists: [],
      explicit: readExplicit(card),
    };
  }

  function collectShelfNodes(sections) {
    const out = [];
    const walk = (node) => {
      if (!node) return;
      if (node.musicCarouselShelfRenderer || node.musicShelfRenderer || node.musicCardShelfRenderer) {
        out.push(node);
        return;
      }
      for (const c of node.itemSectionRenderer?.contents ?? []) walk(c);
      for (const c of node.sectionListRenderer?.contents ?? []) walk(c);
      if (node.gridRenderer?.items) {
        out.push({ musicShelfRenderer: { title: node.gridRenderer.header?.gridHeaderRenderer?.title, contents: node.gridRenderer.items } });
      }
    };
    sections.forEach(walk);
    return out;
  }

  // The raw renderers behind a shelf. `mapShelfWrapper` projects them into the shell's item
  // shape, which drops the menu and navigation endpoints needed to tell an owned playlist from
  // a followed one, so this takes the nodes as they arrived.
  function shelfRendererItems(wrapper) {
    const music =
      wrapper.musicCarouselShelfRenderer ??
      wrapper.musicShelfRenderer ??
      wrapper.musicCardShelfRenderer;
    return (music?.contents ?? music?.items ?? [])
      .map((c) => c.musicTwoRowItemRenderer ?? c.musicResponsiveListItemRenderer)
      .filter(Boolean);
  }

  function mapShelfWrapper(wrapper) {
    const card = wrapper.musicCardShelfRenderer;
    const music = wrapper.musicCarouselShelfRenderer ?? wrapper.musicShelfRenderer ?? card;
    if (!music) return { title: "", items: [] };
    const title = card
      ? readRuns(card.header?.musicCardShelfHeaderBasicRenderer?.title)
      : readRuns(music.header?.musicCarouselShelfBasicHeaderRenderer?.title ?? music.title);
    const items = [];
    for (const c of music.contents ?? []) {
      const mapped = c.musicTwoRowItemRenderer
        ? mapTwoRowItem(c.musicTwoRowItemRenderer)
        : c.musicResponsiveListItemRenderer
          ? mapResponsiveListItem(c.musicResponsiveListItemRenderer)
          : null;
      if (mapped) items.push(mapped);
    }
    if (card) {
      const featured = mapCardShelfFeatured(card);
      if (featured) items.unshift(featured);
    }
    // An untitled shelf takes the page title further up, not a numbered placeholder.
    return { title, items };
  }

  function collectResponsiveRows(root) {
    const out = [];
    const seen = new WeakSet();
    const walk = (node) => {
      if (!node || typeof node !== "object") return;
      if (seen.has(node)) return;
      seen.add(node);
      if (Array.isArray(node)) { for (const c of node) walk(c); return; }
      if (node.musicResponsiveListItemRenderer) out.push(node.musicResponsiveListItemRenderer);
      for (const key of Object.keys(node)) walk(node[key]);
    };
    walk(root);
    return out;
  }

  // ---------------------------------------------------------------------------------------
  // Page shapes (home.ts, library-pagination.ts).
  // ---------------------------------------------------------------------------------------
  function selectedTabContent(json) {
    const single = json?.contents?.singleColumnBrowseResultsRenderer?.tabs ?? [];
    const two = json?.contents?.twoColumnBrowseResultsRenderer?.tabs ?? [];
    const tabs = single.length > 0 ? single : two;
    const tab = tabs.find((entry) => entry?.tabRenderer?.selected) ?? tabs[0];
    return tab?.tabRenderer?.content;
  }

  function parseInitialPage(json) {
    const content = selectedTabContent(json) ?? json?.contents;
    const sectionList = content?.sectionListRenderer;
    if (sectionList) {
      return {
        sections: Array.isArray(sectionList.contents) ? sectionList.contents : [],
        nextCursor: findContinuationToken(sectionList),
        recognized: true,
      };
    }
    if (content?.gridRenderer || content?.musicShelfRenderer || content?.musicCarouselShelfRenderer) {
      return { sections: [content], nextCursor: findContinuationToken(content), recognized: true };
    }
    // Playlist-shaped pages (liked songs) keep their rows in a secondary column.
    const secondary = json?.contents?.twoColumnBrowseResultsRenderer?.secondaryContents;
    if (secondary) {
      return { sections: [secondary], nextCursor: findContinuationToken(secondary), recognized: true };
    }
    return { sections: [], recognized: false };
  }

  const isLooseItem = (node) =>
    !!(node?.musicTwoRowItemRenderer || node?.musicResponsiveListItemRenderer || node?.musicNavigationButtonRenderer);

  function normalizeContinuationItems(items) {
    const sections = [];
    let loose = [];
    const flush = () => {
      if (loose.length === 0) return;
      sections.push({ musicShelfRenderer: { contents: loose } });
      loose = [];
    };
    for (const item of items) {
      if (item?.continuationItemRenderer) continue;
      if (isLooseItem(item)) { loose.push(item); continue; }
      flush();
      sections.push(item);
    }
    flush();
    return sections;
  }

  function parseContinuationPage(json) {
    const modernItems = [];
    const modernSources = [];
    for (const actions of [json?.onResponseReceivedActions ?? [], json?.onResponseReceivedEndpoints ?? [], json?.onResponseReceivedCommands ?? []]) {
      for (const action of actions) {
        const append = action?.appendContinuationItemsAction?.continuationItems ?? action?.reloadContinuationItemsCommand?.continuationItems;
        if (!Array.isArray(append)) continue;
        modernSources.push(action);
        modernItems.push(...append);
      }
    }
    if (modernSources.length > 0) {
      return { sections: normalizeContinuationItems(modernItems), nextCursor: findContinuationToken(modernSources), recognized: true };
    }
    const cc = json?.continuationContents;
    const legacy = [];
    const add = (node, items) => { if (node) legacy.push({ node, items: Array.isArray(items) ? items : [] }); };
    add(cc?.gridContinuation, cc?.gridContinuation?.items ?? cc?.gridContinuation?.contents);
    add(cc?.musicShelfContinuation, cc?.musicShelfContinuation?.contents);
    add(cc?.musicPlaylistShelfContinuation, cc?.musicPlaylistShelfContinuation?.contents);
    add(cc?.sectionListContinuation, cc?.sectionListContinuation?.contents);
    if (legacy.length > 0) {
      return {
        sections: normalizeContinuationItems(legacy.flatMap((c) => c.items)),
        nextCursor: findContinuationToken(legacy.map((c) => c.node)),
        recognized: true,
      };
    }
    return { sections: [], recognized: false };
  }

  // djb2 → base36, so shelf ids stay stable across a refresh that reorders shelves.
  function hashToken(s) {
    let h = 5381;
    for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) >>> 0;
    return h.toString(36);
  }

  // ---------------------------------------------------------------------------------------
  // Projection into the shell's catalog shape.
  // ---------------------------------------------------------------------------------------
  const largestUrl = (thumbnails) => {
    const sorted = [...(thumbnails ?? [])].sort((a, b) => ((a.width ?? 0) * (a.height ?? 0)) - ((b.width ?? 0) * (b.height ?? 0)));
    return sorted.at(-1)?.url ?? null;
  };

  function toWireItem(item) {
    if (!item) return null;
    const playable = item.kind === "song" || item.kind === "video";
    return {
      kind: item.kind,
      id: item.id,
      title: item.title,
      subtitle: item.subtitle ?? "",
      artist: item.artists?.[0]?.name ?? null,
      artistId: item.artists?.[0]?.id ?? null,
      album: item.album ?? null,
      albumId: item.albumId ?? null,
      duration: item.duration || null,
      thumbnail: largestUrl(item.thumbnails),
      videoId: playable ? item.id : (item.playableVideoId ?? null),
      explicit: !!item.explicit,
    };
  }

  function shelvesFrom(sections, tag, fallbackTitle) {
    const titleSeen = new Map();
    const seenItems = new Set();
    const shelves = [];
    collectShelfNodes(sections).forEach((wrapper, i) => {
      const { title, items } = mapShelfWrapper(wrapper);
      // Library responses repeat cards across "Recently added" and the main shelf; keep the
      // first occurrence in server order.
      const wire = items
        .filter((item) => {
          const key = `${item.kind}:${item.id}`;
          if (seenItems.has(key)) return false;
          seenItems.add(key);
          return true;
        })
        .map(toWireItem)
        .filter(Boolean);
      if (wire.length === 0) return;
      const name = title || fallbackTitle;
      const seen = titleSeen.get(name) ?? 0;
      titleSeen.set(name, seen + 1);
      shelves.push({ id: `${tag}-${name}${seen === 0 ? "" : `-${seen}`}`, title: name, items: wire });
    });
    return shelves;
  }

  // `shape` says how the caller intends to render the answer, because the response alone does
  // not settle it: an album and an artist page are both shelves of responsive rows to a parser,
  // but one is a track list to a person and the other is a set of sections. "auto" keeps the
  // original heuristic — a VL-prefixed browse is a playlist and therefore rows.

  // ---------------------------------------------------------------------------------------
  // Track-shaped pages: playlists and albums (playlist.ts, album.ts).
  //
  // These do not go through `parseInitialPage`. A playlist or album arrives in a two-column
  // layout whose selected tab is the header column, so taking the tab's section list — which is
  // what the shelf pages want — finds the artwork and title and none of the tracks. Both of
  // these parsers instead work from the raw response: album layouts vary between single and two
  // column and between shelf wrappers, but the row renderer is the same in all of them, so a
  // tree walk is what holds up.
  // ---------------------------------------------------------------------------------------

  // The header hides under different renderer keys depending on whether the playlist is owned
  // (editable) or community, and in different places in the tree. Walk for the first match
  // rather than enumerating paths.
  function findByKeys(root, keys) {
    const seen = new WeakSet();
    let result;
    const walk = (node) => {
      if (result || !node || typeof node !== "object") return;
      if (seen.has(node)) return;
      seen.add(node);
      if (Array.isArray(node)) { for (const c of node) walk(c); return; }
      for (const key of keys) {
        if (node[key] && typeof node[key] === "object") { result = node[key]; return; }
      }
      for (const k of Object.keys(node)) walk(node[k]);
    };
    walk(root);
    return result;
  }

  const findTrackPageHeader = (json) =>
    findByKeys(json, ["musicDetailHeaderRenderer", "musicResponsiveHeaderRenderer"]) ?? {};

  function findAppendedContinuationItems(json) {
    for (const actions of [
      json?.onResponseReceivedActions ?? [],
      json?.onResponseReceivedEndpoints ?? [],
      json?.onResponseReceivedCommands ?? [],
    ]) {
      for (const action of actions) {
        const items =
          action?.appendContinuationItemsAction?.continuationItems ??
          action?.reloadContinuationItemsCommand?.continuationItems;
        if (Array.isArray(items)) return { contents: items, continuationSource: items };
      }
    }
    return undefined;
  }

  // Only the container the playlist itself owns. A playlist browse also carries recommendation
  // shelves, and a plain walk of the response silently appends those suggestions to the
  // playlist as though the user had added them.
  function findTrackContainer(json) {
    const continuation = json?.continuationContents?.musicPlaylistShelfContinuation
      ?? json?.continuationContents?.musicShelfContinuation;
    if (continuation) {
      return {
        contents: Array.isArray(continuation.contents) ? continuation.contents : [],
        continuationSource: continuation,
      };
    }
    const shelf = findByKeys(json, ["musicPlaylistShelfRenderer"]);
    if (shelf) {
      return {
        contents: Array.isArray(shelf.contents) ? shelf.contents : [],
        continuationSource: shelf,
      };
    }
    return findAppendedContinuationItems(json);
  }

  // The opaque id for this exact occurrence of a track in this playlist. A playlist may hold the
  // same video twice, so removing or moving one needs this rather than the video id.
  function readEntryId(raw) {
    const direct = raw?.playlistItemData?.playlistSetVideoId;
    if (typeof direct === "string" && direct.trim()) return direct;
    for (const item of raw?.menu?.menuRenderer?.items ?? []) {
      const endpoint =
        item?.menuServiceItemRenderer?.serviceEndpoint?.playlistEditEndpoint ??
        item?.menuNavigationItemRenderer?.navigationEndpoint?.playlistEditEndpoint;
      for (const action of endpoint?.actions ?? []) {
        if (typeof action?.setVideoId === "string" && action.setVideoId.trim()) {
          return action.setVideoId;
        }
      }
    }
    return null;
  }

  function trackPage(json, browseId, title, isContinuation) {
    const container = findTrackContainer(json);
    // The fallback is what makes albums work: they use a plain shelf rather than a playlist
    // shelf, so no dedicated container exists and the rows have to be found by walking. A
    // recognized-but-empty playlist container stays empty rather than falling through to this
    // and filling itself with the page's recommendations.
    const rows = container
      ? container.contents
          .map((c) => c.musicResponsiveListItemRenderer)
          .filter(Boolean)
      : collectResponsiveRows(json);

    const seen = new Set();
    const tracks = [];
    for (const row of rows) {
      const mapped = mapResponsiveListItem(row);
      const wire = toWireItem(mapped);
      if (!wire || !wire.videoId) continue;
      const entryId = readEntryId(row);
      const key = entryId ? `set:${entryId}` : `video:${wire.videoId}`;
      if (seen.has(key)) continue;
      seen.add(key);
      tracks.push({ ...wire, entryId });
    }

    const header = isContinuation ? {} : findTrackPageHeader(json);
    const headerTitle = readRuns(header.title);
    const thumbnails = readThumbnails(
      header.thumbnail?.musicThumbnailRenderer?.thumbnail ??
        header.thumbnail?.croppedSquareThumbnailRenderer?.thumbnail ??
        header.thumbnail?.musicThumbnailRenderer ??
        header.thumbnail,
    );
    return {
      id: `personal:${browseId}`,
      title: headerTitle || title,
      subtitle: readRuns(header.subtitle) || readRuns(header.straplineTextOne) || "",
      shelves: [],
      tracks,
      thumbnail: largestUrl(thumbnails),
      // From the container that produced the rows, not from anywhere in the response: a token
      // picked up off a neighbouring recommendation shelf paginates that shelf instead, which
      // reads as a playlist that loads one page and then stops.
      nextCursor: findContinuationToken(container?.continuationSource ?? json) ?? null,
      truncated: false,
    };
  }

  async function browse(browseId, title, continuation, shape) {
    const json = continuation ? await rawBrowseContinuation(continuation) : await rawBrowse(browseId);

    const rowsOnly =
      shape === "tracks" ? true : shape === "shelves" ? false : browseId.startsWith("VL");
    if (rowsOnly) return JSON.stringify(trackPage(json, browseId, title, !!continuation));

    const page = continuation ? parseContinuationPage(json) : parseInitialPage(json);
    if (!page.recognized) throw new Error(`Unrecognized YouTube Music response for ${browseId}`);
    const tag = continuation ? hashToken(continuation) : "init";
    return JSON.stringify({
      id: `personal:${browseId}`,
      title,
      subtitle: "",
      shelves: shelvesFrom(page.sections, tag, title),
      tracks: [],
      thumbnail: null,
      nextCursor: page.nextCursor ?? null,
      truncated: false,
    });
  }


  // ---------------------------------------------------------------------------------------
  // Mutations (mutations.ts). Every one of these needs the authenticated cookie jar; an
  // anonymous call succeeds HTTP-wise and persists nowhere, which is the failure mode these
  // status checks exist to catch.
  // ---------------------------------------------------------------------------------------

  // Browse ids arrive VL-prefixed (`VLPL…`) while every mutating endpoint wants the bare
  // `PL…`. Normalizing here keeps callers from having to know which shape they hold.
  function barePlaylistId(playlistId) {
    const bare = playlistId?.startsWith("VL") ? playlistId.slice(2) : playlistId;
    if (!bare) throw new Error("Missing playlist id");
    return bare;
  }

  // `edit_playlist` answers HTTP 200 even when it refuses the edit — not the owner, stale
  // cookies, an unsupported action. A caller that does not read the envelope status reports
  // success for an edit that did not happen.
  function assertSucceeded(json, what) {
    const status = json?.status;
    if (status && status !== "STATUS_SUCCEEDED") throw new Error(`${what} failed: ${status}`);
  }

  async function editPlaylist(playlistId, actions) {
    const json = await innertubePost("browse/edit_playlist", {
      playlistId: barePlaylistId(playlistId),
      actions,
    });
    assertSucceeded(json, "edit_playlist");
  }

  // The playlists this account created, as opposed to ones it follows. Only real user
  // playlists have browse ids beginning `VLPL`; the auto-generated pseudo-entries ("New
  // playlist", "Episodes for later", liked songs as `LM`) either lack that or cannot be
  // edited by their owner, and offering them as destinations produces a failure at the point
  // the user has already chosen one.
  async function listUserPlaylists() {
    const json = await rawBrowse("FEmusic_liked_playlists");
    const page = parseInitialPage(json);
    const out = [];
    const seen = new Set();
    for (const raw of collectShelfNodes(page.sections).flatMap(shelfRendererItems)) {
      const browseId =
        raw?.navigationEndpoint?.browseEndpoint?.browseId ??
        raw?.menu?.menuRenderer?.items?.[0]?.menuNavigationItemRenderer?.navigationEndpoint
          ?.browseEndpoint?.browseId;
      if (!browseId?.startsWith("VLPL")) continue;
      const id = browseId.slice(2);
      if (seen.has(id)) continue;
      const title =
        readRuns(raw.title) ||
        readRuns(raw.flexColumns?.[0]?.musicResponsiveListItemFlexColumnRenderer?.text) ||
        "";
      if (!title) continue;
      seen.add(id);
      const thumbnails = readThumbnails(
        raw.thumbnailRenderer?.musicThumbnailRenderer?.thumbnail ??
          raw.thumbnail?.musicThumbnailRenderer?.thumbnail ??
          raw.thumbnail,
      );
      out.push({
        id,
        title,
        subtitle:
          readRuns(raw.subtitle) ||
          readRuns(raw.flexColumns?.[1]?.musicResponsiveListItemFlexColumnRenderer?.text) ||
          "",
        thumbnail: largestUrl(thumbnails),
      });
    }
    return out;
  }

  async function createPlaylist({ title, description, privacy, videoIds }) {
    const name = (title ?? "").trim();
    if (!name) throw new Error("Playlist name cannot be empty");
    const body = { title: name, privacyStatus: privacy || "PRIVATE" };
    if (description) body.description = description;
    if (videoIds?.length) body.videoIds = videoIds;
    const json = await innertubePost("playlist/create", body);
    const id = json?.playlistId ?? json?.response?.playlistId;
    if (!id) throw new Error("Could not read new playlistId from response");
    return { playlistId: id };
  }

  const MUTATIONS = {
    // Ratings. YouTube Music models saving a playlist or an album as a rating on it rather
    // than as a library edit, which is why both reuse the track endpoints.
    rateTrack: ({ videoId, status }) => {
      const endpoint =
        status === "LIKE" ? "like/like" : status === "DISLIKE" ? "like/dislike" : "like/removelike";
      if (!videoId) throw new Error("Missing video id");
      return innertubePost(endpoint, { target: { videoId } });
    },
    ratePlaylist: ({ playlistId, saved }) =>
      innertubePost(saved ? "like/like" : "like/removelike", {
        target: { playlistId: barePlaylistId(playlistId) },
      }),
    setSubscription: ({ channelId, subscribed }) => {
      if (!channelId) throw new Error("Missing channel id");
      return innertubePost(
        subscribed ? "subscription/subscribe" : "subscription/unsubscribe",
        { channelIds: [channelId] },
      );
    },

    // Playlist contents.
    addToPlaylist: ({ playlistId, videoId }) => {
      if (!videoId) throw new Error("Missing video id");
      return editPlaylist(playlistId, [{ action: "ACTION_ADD_VIDEO", addedVideoId: videoId }]);
    },
    // `setVideoId` is an opaque per-entry identifier from a playlist browse. A bare video id
    // would be ambiguous in a playlist that contains the same track twice.
    removeFromPlaylist: ({ playlistId, videoId, setVideoId }) => {
      if (!videoId || !setVideoId) throw new Error("Cannot remove a playlist entry without exact identifiers");
      return editPlaylist(playlistId, [
        { action: "ACTION_REMOVE_VIDEO", removedVideoId: videoId, setVideoId },
      ]);
    },
    movePlaylistItem: ({ playlistId, setVideoId, predecessorSetVideoId }) => {
      if (!setVideoId) throw new Error("Cannot move a playlist entry without its entry id");
      const action = { action: "ACTION_MOVE_VIDEO_BEFORE", setVideoId };
      // Omitted entirely to move an entry to the front; an empty predecessor is not the same
      // request as no predecessor.
      if (predecessorSetVideoId) action.movedSetVideoIdPredecessor = predecessorSetVideoId;
      return editPlaylist(playlistId, [action]);
    },
    addPlaylistToPlaylist: ({ playlistId, sourcePlaylistId }) =>
      editPlaylist(playlistId, [
        { action: "ACTION_ADD_PLAYLIST", addedFullListId: barePlaylistId(sourcePlaylistId) },
      ]),

    // Playlist itself.
    createPlaylist,
    renamePlaylist: ({ playlistId, title }) => {
      const trimmed = (title ?? "").trim();
      if (!trimmed) throw new Error("Playlist name cannot be empty");
      return editPlaylist(playlistId, [
        { action: "ACTION_SET_PLAYLIST_NAME", playlistName: trimmed },
      ]);
    },
    setPlaylistDescription: ({ playlistId, description }) =>
      editPlaylist(playlistId, [
        { action: "ACTION_SET_PLAYLIST_DESCRIPTION", playlistDescription: description ?? "" },
      ]),
    setPlaylistPrivacy: ({ playlistId, privacy }) =>
      editPlaylist(playlistId, [
        { action: "ACTION_SET_PLAYLIST_PRIVACY", playlistPrivacy: privacy },
      ]),
    // No undo exists upstream, so the caller confirms before reaching here.
    deletePlaylist: async ({ playlistId }) => {
      const json = await innertubePost("playlist/delete", { playlistId: barePlaylistId(playlistId) });
      assertSucceeded(json, "playlist/delete");
    },

    listUserPlaylists: () => listUserPlaylists(),
  };

  // One entry point, so the host has one shape to call and one shape to decode. The result is
  // always an object: an operation with nothing to report answers `{}` rather than undefined,
  // because "no value" and "no answer" have to look different to the decoder.
  async function mutate(operation, args) {
    const run = MUTATIONS[operation];
    if (!run) throw new Error(`Unknown mutation ${operation}`);
    const value = await run(args ?? {});
    return JSON.stringify(value && typeof value === "object" && !Array.isArray(value)
      ? value
      : { value: value ?? null });
  }

  return { browse, mutate };
})();
