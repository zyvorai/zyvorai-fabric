import type {ReactNode} from 'react';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';

/**
 * A diagram from docs/assets/keep, served in place (see staticDirectories in docusaurus.config.ts) so this page and
 * the GitHub docs show the same file. Wide diagrams scroll sideways on a phone rather than shrinking to nothing.
 */
export default function Figure({
  file,
  alt,
  caption,
  minWidth = 760,
}: {
  file: string;
  alt: string;
  caption?: ReactNode;
  minWidth?: number;
}): ReactNode {
  const src = useBaseUrl(`/merion/${file}`);
  return (
    <figure className={styles.figure}>
      <div className={styles.scroller} tabIndex={0} role="group" aria-label={alt}>
        <img src={src} alt={alt} loading="lazy" style={{minWidth}} />
      </div>
      <figcaption>
        {caption}
        {caption ? ' ' : null}
        <a href={src} target="_blank" rel="noreferrer">
          Open full size
        </a>
      </figcaption>
    </figure>
  );
}
