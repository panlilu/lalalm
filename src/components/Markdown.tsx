// Tiny safe markdown → JSX renderer (no HTML passthrough).
import { Fragment, type ReactNode } from "react";
import type { MouseEvent } from "react";

function renderInline(text: string, keyBase: string): ReactNode[] {
  // Token order matters: images → links → code → bold → italic.
  const tokens: ReactNode[] = [];
  let rest = text;
  let k = 0;

  const pushPlain = (s: string) => {
    if (s) tokens.push(<Fragment key={`${keyBase}-p${k++}`}>{s}</Fragment>);
  };

  const patterns: Array<[RegExp, (m: RegExpMatchArray) => ReactNode]> = [
    [/^!\[([^\]]*)\]\(([^)\s]+)[^)]*\)/, (m) => (
      <span
        key={`img-${k}`}
        className="md-link"
        title={m[2]}
        onClick={(e: MouseEvent) => {
          e.preventDefault();
          navigator.clipboard?.writeText(m[2]);
        }}
      >
        🖼 [图片]
      </span>
    )],
    [/^\[([^\]]*)\]\(([^)\s]+)[^)]*\)/, (m) => (
      <span
        key={`lnk-${k}`}
        className="md-link"
        title={m[2]}
        onClick={(e: MouseEvent) => {
          e.preventDefault();
          navigator.clipboard?.writeText(m[2]);
        }}
      >
        {m[1] || m[2]}
      </span>
    )],
    [/^`([^`]+)`/, (m) => <code key={`c-${k}`}>{m[1]}</code>],
    [/^\*\*([^*]+)\*\*/, (m) => <strong key={`b-${k}`}>{m[1]}</strong>],
    [/^__([^_]+)__/, (m) => <strong key={`b-${k}`}>{m[1]}</strong>],
    [/^\*([^*\n]+)\*/, (m) => <em key={`i-${k}`}>{m[1]}</em>],
  ];

  while (rest.length > 0) {
    let matched = false;
    for (const [re, fn] of patterns) {
      const m = rest.match(re);
      if (m) {
        pushPlain(rest.slice(0, m.index ?? 0));
        tokens.push(fn(m));
        rest = rest.slice((m.index ?? 0) + m[0].length);
        matched = true;
        break;
      }
    }
    if (!matched) {
      // consume one char to guarantee progress
      const nextSpecial = rest.slice(1).search(/[!*`[]/);
      const take = nextSpecial === -1 ? rest.length : nextSpecial + 1;
      pushPlain(rest.slice(0, take));
      rest = rest.slice(take);
    }
  }
  return tokens;
}

export function Markdown({
  source,
}: {
  source: string;
  onCopyLink?: (t: string) => void;
}) {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const out: ReactNode[] = [];
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const line = lines[i];

    // fenced code block
    if (line.trimStart().startsWith("```")) {
      const buf: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        buf.push(lines[i]);
        i++;
      }
      i++; // closing fence
      out.push(
        <pre key={key++}>
          <code>{buf.join("\n")}</code>
        </pre>
      );
      continue;
    }

    // heading
    const hm = line.match(/^(#{1,6})\s+(.*)/);
    if (hm) {
      const level = Math.min(hm[1].length, 4);
      const Tag = (`h${level}` as unknown) as "h1";
      out.push(<Tag key={key++}>{renderInline(hm[2], `h${key}`)}</Tag>);
      i++;
      continue;
    }

    // hr
    if (/^\s*(---+|\*\*\*+)\s*$/.test(line)) {
      out.push(<hr key={key++} />);
      i++;
      continue;
    }

    // table
    if (
      line.includes("|") &&
      i + 1 < lines.length &&
      /^\s*\|?[\s:-]*-[-\s|:]*\|?\s*$/.test(lines[i + 1])
    ) {
      const parseRow = (l: string) =>
        l.replace(/^\s*\|/, "").replace(/\|\s*$/, "").split("|").map((c) => c.trim());
      const header = parseRow(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
        rows.push(parseRow(lines[i]));
        i++;
      }
      out.push(
        <table key={key++}>
          <thead>
            <tr>
              {header.map((h, hi) => (
                <th key={hi}>{renderInline(h, `th${hi}`)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, ri) => (
              <tr key={ri}>
                {r.map((c, ci) => (
                  <td key={ci}>{renderInline(c, `td${ri}-${ci}`)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
      continue;
    }

    // blockquote
    if (line.trimStart().startsWith(">")) {
      const buf: string[] = [];
      while (i < lines.length && lines[i].trimStart().startsWith(">")) {
        buf.push(lines[i].replace(/^\s*>\s?/, ""));
        i++;
      }
      out.push(
        <blockquote key={key++}>
          {buf.map((b, bi) => (
            <Fragment key={bi}>
              {renderInline(b, `q${bi}`)}
              <br />
            </Fragment>
          ))}
        </blockquote>
      );
      continue;
    }

    // lists
    if (/^\s*([-*+]|\d+\.)\s+/.test(line)) {
      const ordered = /^\s*\d+\./.test(line);
      const items: ReactNode[] = [];
      while (i < lines.length && /^\s*([-*+]|\d+\.)\s+/.test(lines[i])) {
        const content = lines[i].replace(/^\s*([-*+]|\d+\.)\s+/, "");
        items.push(<li key={items.length}>{renderInline(content, `li${i}`)}</li>);
        i++;
      }
      out.push(
        ordered ? <ol key={key++}>{items}</ol> : <ul key={key++}>{items}</ul>
      );
      continue;
    }

    // blank
    if (line.trim() === "") {
      i++;
      continue;
    }

    // paragraph
    const buf: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^(#{1,6})\s/.test(lines[i]) &&
      !lines[i].trimStart().startsWith("```") &&
      !lines[i].trimStart().startsWith(">") &&
      !/^\s*([-*+]|\d+\.)\s+/.test(lines[i])
    ) {
      buf.push(lines[i]);
      i++;
    }
    out.push(
      <p key={key++}>
        {buf.map((b, bi) => (
          <Fragment key={bi}>
            {renderInline(b, `p${key}-${bi}`)}
            {bi < buf.length - 1 ? <br /> : null}
          </Fragment>
        ))}
      </p>
    );
  }

  return <div className="md-body">{out}</div>;
}
