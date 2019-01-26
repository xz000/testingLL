using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class CentrallyConstentField : MonoBehaviour
{
    public Rigidbody2D center;
    public GameObject sender;
    public float speed;

    public void AddConstentCentrallyVelocity(Rigidbody2D victim,MoveScript worker)
    {
        Vector2 vector = center.position - victim.position;
        if (vector.sqrMagnitude < 0.01)
            return;
        worker.VelotoAdd += (center.position - victim.position).normalized * speed;
    }

    private void OnTriggerEnter2D(Collider2D collision)
    {
        if (collision.gameObject == sender)
            return;
        MoveScript MS = collision.GetComponent<MoveScript>();
        MS.cook += AddConstentCentrallyVelocity;
    }

    private void OnTriggerExit2D(Collider2D collision)
    {
        MoveScript MS = collision.GetComponent<MoveScript>();
        MS.cook -= AddConstentCentrallyVelocity;
    }

    public void setspeed(float spd)
    {
        speed = spd;
    }
}
