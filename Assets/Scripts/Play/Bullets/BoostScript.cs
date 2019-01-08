using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class BoostScript : MonoBehaviour
{
    public GameObject sender;
    public float maxtime = 2;
    float timepsd = 0;
    
    void OnDestroy()
    {
        if (/*photonView.isMine && */sender != null)
        {
            sender.GetComponent<HPScript>().boostend();
        }
    }
    
    void Update()
    {
        transform.position = sender.transform.position;
        if (timepsd >= maxtime || sender == null)
            gameObject.GetComponent<DestroyScript>().Destroyself();
    }

    void FixedUpdate()
    {
        timepsd += Time.fixedDeltaTime;
    }
}
