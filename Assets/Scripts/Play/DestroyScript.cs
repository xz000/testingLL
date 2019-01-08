using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class DestroyScript : MonoBehaviour
{
    public bool breakable = false;
    public bool selfprotect = true;

    public void Destroyself()
    {
        Destroy(gameObject);
    }
}
