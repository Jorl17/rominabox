/* A redirect goes through net_http_connection_new before any socket opens.
 * Prove that in the test build we reject an external Location there. */
#include <assert.h>
#include <stdbool.h>
#include <stdlib.h>

#include <net/net_http.h>

static bool accepts(const char *url)
{
   struct http_connection_t *connection = net_http_connection_new(url, "GET", NULL);
   bool valid;
   if (!connection)
      return false;
   valid = net_http_connection_iterate(connection) &&
      net_http_connection_done(connection);
   net_http_connection_free(connection);
   return valid;
}

int main(void)
{
   assert(setenv("ROMINABOX_RA_TEST_HOST", "http://127.0.0.1:42123", 1) == 0);
   assert(accepts("http://127.0.0.1:42123/dorequest.php"));
   assert(!accepts("https://retroachievements.org/dorequest.php"));
   assert(!accepts("http://127.0.0.1:42124/dorequest.php"));
   assert(!accepts("http://127.0.0.1:42123.evil.test/dorequest.php"));
   assert(!accepts("http://localhost:42123/dorequest.php"));
   assert(setenv("ROMINABOX_RA_TEST_HOST", "https://retroachievements.org", 1) == 0);
   assert(!accepts("https://retroachievements.org/dorequest.php"));
   return 0;
}
