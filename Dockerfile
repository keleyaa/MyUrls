ARG NODE_IMAGE=node:24.14.1-alpine@sha256:8510330d3eb72c804231a834b1a8ebb55cb3796c3e4431297a24d246b8add4d5

FROM ${NODE_IMAGE} AS dependencies
WORKDIR /app
ENV COREPACK_HOME=/tmp/corepack
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY apps/server/package.json apps/server/package.json
COPY apps/web/package.json apps/web/package.json
COPY packages/contracts/package.json packages/contracts/package.json
RUN pnpm install --frozen-lockfile

FROM dependencies AS build
COPY . .
RUN pnpm build
RUN pnpm --filter @myurl/server deploy --prod --legacy /tmp/server-deploy

FROM ${NODE_IMAGE} AS runtime
ENV NODE_ENV=production
ENV WEB_ROOT=/app/apps/web/dist
WORKDIR /app
RUN apk upgrade --no-cache && \
    rm -rf /usr/local/lib/node_modules/npm /usr/local/bin/npm /usr/local/bin/npx
COPY --from=build --chown=node:node /tmp/server-deploy/node_modules ./node_modules
COPY --from=build --chown=node:node /tmp/server-deploy/package.json ./package.json
COPY --from=build --chown=node:node /app/apps/server/dist ./apps/server/dist
COPY --from=build --chown=node:node /app/apps/web/dist ./apps/web/dist
USER node
EXPOSE 3000
CMD ["node", "apps/server/dist/index.js"]
