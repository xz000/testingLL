using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class RedLineScript : MonoBehaviour
{
    public Rigidbody2D sender;
    public Rigidbody2D receiver;
    Vector2 centerpoint;
    public LineRenderer MyLine;
    public float speed = 2;
    public Fix64 damage;
    public float maxtime = 2;
    float timepsd = 0;
    //public bool Idrag = false;
    bool pointalive = false;

    // Use this for initialization
    void Start()
    {
        timepsd = 0;
    }

    // Update is called once per frame
    void Update()
    {
        drawmyline(sender.position, centerpoint);
    }

    void FixedUpdate()
    {
        timepsd += Time.fixedDeltaTime;
        if (pointalive)
        {
            centerpoint = receiver.position;
            receiver.GetComponent<HPScript>().GetHurt(damage * (Fix64)Time.fixedDeltaTime);
        }
        Vector2 RayV2 = sender.position - centerpoint;
        RaycastHit2D[] Allhit = Physics2D.RaycastAll(centerpoint, RayV2);
        foreach (RaycastHit2D hit in Allhit)
        {
            if (hit.collider.gameObject == sender.gameObject || hit.collider.GetComponent<HPScript>() == null)//!hit.collider.gameObject.GetPhotonView().isMine || 
                continue;
            hit.collider.GetComponent<HPScript>().GetHurt(damage * (Fix64)Time.fixedDeltaTime);
        }
        if (timepsd >= maxtime || sender == null)
            gameObject.GetComponent<DestroyScript>().Destroyself();
    }

    public void SetRSC(float spd, Fix64 dmg, float maxT)
    {
        damage = dmg;
        speed = spd;
        maxtime = maxT;
    }

    public void RedLineWorking(Rigidbody2D victimId)
    {
        receiver = victimId;
        sender.GetComponent<MoveScript>().cook += AddConstentCentrallyVelocity;
        pointalive = true;
        centerpoint = receiver.position;
        receiver.GetComponent<DoSkill>().ClearDebuff += gameObject.GetComponent<DestroyScript>().Destroyself;
        enabled = true;
    }

    public void RedLineMissed(Vector2 missedplace)
    {
        centerpoint = missedplace;
        enabled = true;
    }

    public void AddConstentCentrallyVelocity(Rigidbody2D victim, MoveScript worker)
    {
        Vector2 distance = receiver.position - victim.position;
        if (distance.sqrMagnitude > 1.1)
            worker.VelotoAdd += distance.normalized * speed;
    }

    void OnDestroy()
    {
        if (sender != null)
            sender.GetComponent<MoveScript>().cook -= AddConstentCentrallyVelocity;
        if (receiver != null)
            receiver.GetComponent<DoSkill>().ClearDebuff -= gameObject.GetComponent<DestroyScript>().Destroyself;
    }

    void drawmyline(Vector2 v21, Vector2 v22)
    {
        MyLine.SetPosition(0, v21);
        MyLine.SetPosition(1, v22);
    }
}
