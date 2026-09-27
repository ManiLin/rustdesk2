FROM node:22-alpine AS build
WORKDIR /app
COPY package.json ./
RUN npm install
COPY . .
RUN npm run build

FROM nginx:alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
# Консоль управления живёт в /managment; в корне сайта нет — только пустая 404-заглушка.
COPY --from=build /app/dist /usr/share/nginx/html/managment
COPY 404.html /usr/share/nginx/html/404.html
EXPOSE 80
