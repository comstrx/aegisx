/* Loopback-only benchmark fixture; fixed GET response, not an application server. */
#define _GNU_SOURCE
#include <arpa/inet.h>
#include <errno.h>
#include <netinet/tcp.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/epoll.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>
typedef struct { int fd; size_t used, sent; int writing; char data[8192]; } Client;
static const char response[]="HTTP/1.1 200 OK\r\nContent-Length: 128\r\nContent-Type: text/plain\r\nConnection: keep-alive\r\n\r\n"
"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
static void discard(int ep,Client *c) { epoll_ctl(ep,EPOLL_CTL_DEL,c->fd,NULL);close(c->fd);free(c); }
static void worker(int port) {
 int listener=socket(AF_INET,SOCK_STREAM|SOCK_NONBLOCK,0),one=1;
 setsockopt(listener,SOL_SOCKET,SO_REUSEADDR,&one,sizeof(one));
 setsockopt(listener,SOL_SOCKET,SO_REUSEPORT,&one,sizeof(one));
 struct sockaddr_in addr={.sin_family=AF_INET,.sin_port=htons(port),.sin_addr.s_addr=htonl(INADDR_LOOPBACK)};
 if(bind(listener,(void*)&addr,sizeof(addr))||listen(listener,4096)) {perror("listen");exit(1);}
 int ep=epoll_create1(0);struct epoll_event event={.events=EPOLLIN,.data.ptr=NULL};
 epoll_ctl(ep,EPOLL_CTL_ADD,listener,&event);
 struct epoll_event events[256];
 for(;;) {
  int count=epoll_wait(ep,events,256,-1);
  for(int i=0;i<count;i++) {
   Client *c=events[i].data.ptr;
   if(!c) {
    int fd;
    while((fd=accept4(listener,NULL,NULL,SOCK_NONBLOCK|SOCK_CLOEXEC))>=0) {
     setsockopt(fd,IPPROTO_TCP,TCP_NODELAY,&one,sizeof(one));
     c=calloc(1,sizeof(*c)); if(!c){close(fd);continue;} c->fd=fd;
     event=(struct epoll_event){.events=EPOLLIN|EPOLLRDHUP,.data.ptr=c};epoll_ctl(ep,EPOLL_CTL_ADD,fd,&event);
    }
    continue;
   }
   if(events[i].events&(EPOLLERR|EPOLLHUP|EPOLLRDHUP)) {discard(ep,c);continue;}
   if(!c->writing) {
    ssize_t n=recv(c->fd,c->data+c->used,sizeof(c->data)-c->used,0);
    if(n<=0) {if(n==0||errno!=EAGAIN)discard(ep,c);continue;}
    c->used+=(size_t)n;
    char *end=memmem(c->data,c->used,"\r\n\r\n",4);
    if(!end) {if(c->used==sizeof(c->data))discard(ep,c);continue;}
    if(c->used<4||memcmp(c->data,"GET ",4)) {discard(ep,c);continue;}
    /* The benchmark has one request outstanding per connection; reject pipelining. */
    if((size_t)(end-c->data)+4!=c->used) {discard(ep,c);continue;}
    c->writing=1;c->sent=0;
   }
   ssize_t n=send(c->fd,response+c->sent,sizeof(response)-1-c->sent,MSG_NOSIGNAL);
   if(n<0&&errno!=EAGAIN) {discard(ep,c);continue;}
   if(n>0)c->sent+=(size_t)n;
   if(c->sent==sizeof(response)-1) {c->writing=0;c->used=0;}
   event=(struct epoll_event){.events=(c->writing?EPOLLOUT:EPOLLIN)|EPOLLRDHUP,.data.ptr=c};
   epoll_ctl(ep,EPOLL_CTL_MOD,c->fd,&event);
  }
 }
}
int main(int argc,char **argv) {
 if(argc!=3)return 2;
 int port=atoi(argv[1]),workers=atoi(argv[2]);if(port<1024||workers<1||workers>8)return 2;
 signal(SIGPIPE,SIG_IGN);
 for(int i=1;i<workers;i++) {pid_t pid=fork();if(pid==0){worker(port);return 0;}if(pid<0)return 1;}
 worker(port);return 0;
}
