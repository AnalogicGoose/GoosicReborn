/*
 * PersonalCatalog.js — account-scoped YouTube Music reads, run inside the signed-in
 * WebKit profile by PersonalCatalogHost.
 *
 * Ported from the previous Goosic application, src/lib/innertube/{shared,home,library,
 * library-pagination,types}.ts, and adapted to run as a page program: the request context
 * and API key come from the page's own `ytcfg`, the session cookie never leaves the
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

  async function browse(browseId, title, continuation) {
    const json = continuation ? await rawBrowseContinuation(continuation) : await rawBrowse(browseId);
    const page = continuation ? parseContinuationPage(json) : parseInitialPage(json);
    if (!page.recognized) throw new Error(`Unrecognized YouTube Music response for ${browseId}`);
    const tag = continuation ? hashToken(continuation) : "init";

    // Playlist-shaped browses (liked songs) are a flat list of rows, not shelves.
    const rowsOnly = browseId.startsWith("VL");
    const shelves = rowsOnly ? [] : shelvesFrom(page.sections, tag, title);
    const tracks = rowsOnly
      ? collectResponsiveRows(page.sections).map(mapResponsiveListItem).map(toWireItem).filter((item) => item && item.videoId)
      : [];

    return JSON.stringify({
      id: `personal:${browseId}`,
      title,
      subtitle: "",
      shelves,
      tracks,
      thumbnail: null,
      nextCursor: page.nextCursor ?? null,
      truncated: false,
    });
  }

  return { browse };
})();
